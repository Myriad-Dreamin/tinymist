//! tinymist LSP mode

use core::fmt;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use std::{collections::HashMap, path::PathBuf};

use anyhow::Context;
use crossbeam_channel::select;
use crossbeam_channel::Receiver;
use futures::future::BoxFuture;
use log::{error, info, trace, warn};
use lsp_server::{ErrorCode, Message, Notification, Request, RequestId, ResponseError};
use lsp_types::notification::Notification as NotificationTrait;
use lsp_types::request::{GotoDeclarationParams, GotoDeclarationResponse, WorkspaceConfiguration};
use lsp_types::*;
use parking_lot::lock_api::RwLock;
use paste::paste;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};
use tinymist_query::{
    get_semantic_tokens_options, get_semantic_tokens_registration,
    get_semantic_tokens_unregistration, ExportKind, PageSelection, SemanticTokenContext,
};
use tokio::sync::mpsc;
use typst::diag::StrResult;
use typst::syntax::package::{PackageSpec, VersionlessPackageSpec};
use typst::util::Deferred;
use typst_ts_compiler::service::Compiler;
use typst_ts_core::path::PathClean;
use typst_ts_core::{error::prelude::*, ImmutPath};

use super::lsp_init::*;
use crate::actor::cluster::CompileClusterRequest;
use crate::actor::typ_client::CompileClientActor;
use crate::actor::{FormattingConfig, FormattingRequest};
use crate::compiler::{CompileServer, CompileServerArgs};
use crate::compiler_init::CompilerConstConfig;
use crate::harness::{InitializedLspDriver, LspHost};
use crate::tools::package::InitTask;
use crate::world::SharedFontResolver;
use crate::{run_query, LspResult};

pub type MaySyncResult<'a> = Result<JsonValue, BoxFuture<'a, JsonValue>>;

#[derive(Debug)]
enum Event {
    Lsp(lsp_server::Message),
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::Lsp(_) => write!(f, "Event::Lsp"),
        }
    }
}

pub(crate) struct Cancelled;

type LspMethod<Res> = fn(srv: &mut TypstLanguageServer, args: JsonValue) -> LspResult<Res>;
type LspHandler<Req, Res> = fn(srv: &mut TypstLanguageServer, args: Req) -> LspResult<Res>;

/// Returns Ok(Some()) -> Already responded
/// Returns Ok(None) -> Need to respond none
/// Returns Err(..) -> Need to respond error
type LspRawHandler =
    fn(srv: &mut TypstLanguageServer, args: (RequestId, JsonValue)) -> LspResult<Option<()>>;

type ExecuteCmdMap = HashMap<&'static str, LspHandler<Vec<JsonValue>, JsonValue>>;
type NotifyCmdMap = HashMap<&'static str, LspMethod<()>>;
type RegularCmdMap = HashMap<&'static str, LspRawHandler>;
type ResourceMap = HashMap<ImmutPath, LspHandler<Vec<JsonValue>, JsonValue>>;

macro_rules! exec_fn {
    ($ty: ty, Self::$method: ident, $($arg_key:ident),+ $(,)?) => {{
        const E: $ty = |this, $($arg_key),+| this.$method($($arg_key),+);
        E
    }};
}

macro_rules! request_fn_ {
    ($desc: ty, Self::$method: ident) => {
        (<$desc>::METHOD, {
            const E: LspRawHandler = |this, (req_id, req)| {
                let req: <$desc as lsp_types::request::Request>::Params =
                    serde_json::from_value(req).unwrap(); // todo: soft unwrap
                this.$method(req_id, req)
            };
            E
        })
    };
}

macro_rules! request_fn {
    ($desc: ty, Self::$method: ident) => {
        (<$desc>::METHOD, {
            const E: LspRawHandler = |this, (req_id, req)| {
                let req: <$desc as lsp_types::request::Request>::Params =
                    serde_json::from_value(req).unwrap(); // todo: soft unwrap
                let res = this
                    .$method(req)
                    .map(|res| serde_json::to_value(res).unwrap()); // todo: soft unwrap

                if let Ok(response) = result_to_response(req_id, res) {
                    this.client.respond(response);
                }

                // todo: cancellation
                // Err(e) => match e.downcast::<Cancelled>() {
                //     Ok(cancelled) => return Err(cancelled),
                //     Err(e) => lsp_server::Response::new_err(
                //         id,
                //         lsp_server::ErrorCode::InternalError as i32,
                //         e.to_string(),
                //     ),
                // },

                Ok(Some(()))
            };
            E
        })
    };
}

macro_rules! notify_fn {
    ($desc: ty, Self::$method: ident) => {
        (<$desc>::METHOD, {
            const E: LspMethod<()> = |this, input| {
                let input: <$desc as lsp_types::notification::Notification>::Params =
                    serde_json::from_value(input).unwrap(); // todo: soft unwrap
                this.$method(input)
            };
            E
        })
    };
}

fn as_path(inp: TextDocumentIdentifier) -> PathBuf {
    as_path_(inp.uri)
}

fn as_path_(uri: Url) -> PathBuf {
    tinymist_query::url_to_path(uri)
}

fn as_path_pos(inp: TextDocumentPositionParams) -> (PathBuf, Position) {
    (as_path(inp.text_document), inp.position)
}

pub struct TypstLanguageServerArgs {
    pub client: LspHost<TypstLanguageServer>,
    pub const_config: ConstConfig,
    pub diag_tx: mpsc::UnboundedSender<CompileClusterRequest>,
    pub font: Deferred<SharedFontResolver>,
}

/// The object providing the language server functionality.
pub struct TypstLanguageServer {
    /// The language server client.
    pub client: LspHost<TypstLanguageServer>,

    // State to synchronize with the client.
    /// Whether the server is shutting down.
    pub shutdown_requested: bool,
    /// Whether the server has registered semantic tokens capabilities.
    pub sema_tokens_registered: Option<bool>,
    /// Whether the server has registered document formatter capabilities.
    pub formatter_registered: Option<bool>,
    /// Whether client is pinning a file.
    pub pinning: bool,
    /// The client focusing file.
    pub focusing: Option<ImmutPath>,

    // Configurations
    /// User configuration from the editor.
    pub config: Config,
    /// Const configuration initialized at the start of the session.
    /// For example, the position encoding.
    pub const_config: ConstConfig,

    // Command maps
    /// Extra commands provided with `textDocument/executeCommand`.
    pub exec_cmds: ExecuteCmdMap,
    /// Regular notifications for dispatching.
    pub notify_cmds: NotifyCmdMap,
    /// Regular commands for dispatching.
    pub regular_cmds: RegularCmdMap,
    /// Regular commands for dispatching.
    pub resources_routes: ResourceMap,

    // Resources
    /// The semantic token context.
    pub tokens_ctx: SemanticTokenContext,
    /// The compiler for general purpose.
    pub primary: CompileServer,
    /// The compilers for tasks
    pub dedicates: Vec<CompileServer>,
    /// The formatter thread running in backend.
    /// Note: The thread will exit if you drop the sender.
    pub format_thread: Option<crossbeam_channel::Sender<FormattingRequest>>,
}

/// Getters and the main loop.
impl TypstLanguageServer {
    /// Create a new language server.
    pub fn new(args: TypstLanguageServerArgs) -> Self {
        let tokens_ctx = SemanticTokenContext::new(
            args.const_config.position_encoding,
            args.const_config.sema_tokens_overlapping_token_support,
            args.const_config.sema_tokens_multiline_token_support,
        );
        Self {
            client: args.client.clone(),
            primary: CompileServer::new(CompileServerArgs {
                client: LspHost::new(Arc::new(RwLock::new(None))),
                compile_config: Default::default(),
                const_config: CompilerConstConfig {
                    position_encoding: args.const_config.position_encoding,
                },
                diag_tx: args.diag_tx,
                font: args.font,
                handle: tokio::runtime::Handle::current(),
            }),
            dedicates: Vec::new(),
            shutdown_requested: false,
            sema_tokens_registered: None,
            formatter_registered: None,
            config: Default::default(),
            const_config: args.const_config,

            exec_cmds: Self::get_exec_commands(),
            regular_cmds: Self::get_regular_cmds(),
            notify_cmds: Self::get_notify_cmds(),
            resources_routes: Self::get_resources_routes(),

            pinning: false,
            focusing: None,
            tokens_ctx,
            format_thread: None,
        }
    }

    /// Get the const configuration.
    pub fn const_config(&self) -> &ConstConfig {
        &self.const_config
    }

    /// Get the primary compiler for those commands without task context.
    pub fn primary(&self) -> &CompileClientActor {
        self.primary.compiler.as_ref().expect("primary")
    }

    #[rustfmt::skip]
    fn get_regular_cmds() -> RegularCmdMap {
        use lsp_types::request::*;
        RegularCmdMap::from_iter([
            request_fn!(Shutdown, Self::shutdown),
            // lantency sensitive
            request_fn!(Completion, Self::completion),
            request_fn!(SemanticTokensFullRequest, Self::semantic_tokens_full),
            request_fn!(SemanticTokensFullDeltaRequest, Self::semantic_tokens_full_delta),
            request_fn!(DocumentSymbolRequest, Self::document_symbol),
            // Sync for low latency
            request_fn_!(Formatting, Self::formatting),
            request_fn!(SelectionRangeRequest, Self::selection_range),
            // latency insensitive
            request_fn!(InlayHintRequest, Self::inlay_hint),
            request_fn!(HoverRequest, Self::hover),
            request_fn!(CodeLensRequest, Self::code_lens),
            request_fn!(FoldingRangeRequest, Self::folding_range),
            request_fn!(SignatureHelpRequest, Self::signature_help),
            request_fn!(PrepareRenameRequest, Self::prepare_rename),
            request_fn!(Rename, Self::rename),
            request_fn!(GotoDefinition, Self::goto_definition),
            request_fn!(GotoDeclaration, Self::goto_declaration),
            request_fn!(References, Self::references),
            request_fn!(WorkspaceSymbolRequest, Self::symbol),
            request_fn!(ExecuteCommand, Self::execute_command),
        ])
    }

    fn get_notify_cmds() -> NotifyCmdMap {
        // todo: .on_sync_mut::<notifs::Cancel>(handlers::handle_cancel)?
        use lsp_types::notification::*;
        NotifyCmdMap::from_iter([
            notify_fn!(DidOpenTextDocument, Self::did_open),
            notify_fn!(DidCloseTextDocument, Self::did_close),
            notify_fn!(DidChangeTextDocument, Self::did_change),
            notify_fn!(DidSaveTextDocument, Self::did_save),
            notify_fn!(DidChangeConfiguration, Self::did_change_configuration),
        ])
    }
}

impl InitializedLspDriver for TypstLanguageServer {
    /// The [`initialized`] notification is sent from the client to the server
    /// after the client received the result of the initialize request but
    /// before the client sends anything else.
    ///
    /// [`initialized`]: https://microsoft.github.io/language-server-protocol/specification#initialized
    ///
    /// The server can use the `initialized` notification, for example, to
    /// dynamically register capabilities with the client.
    fn initialized(&mut self, params: InitializedParams) {
        if self.const_config().sema_tokens_dynamic_registration
            && self.config.semantic_tokens == SemanticTokensMode::Enable
        {
            let err = self.enable_sema_token_caps(true);
            if let Err(err) = err {
                error!("could not register semantic tokens for initialization: {err}");
            }
        }

        if self.const_config().doc_fmt_dynamic_registration
            && self.config.formatter != FormatterMode::Disable
        {
            let err = self.enable_formatter_caps(true);
            if let Err(err) = err {
                error!("could not register formatter for initialization: {err}");
            }
        }

        if self.const_config().cfg_change_registration {
            trace!("setting up to request config change notifications");

            const CONFIG_REGISTRATION_ID: &str = "config";
            const CONFIG_METHOD_ID: &str = "workspace/didChangeConfiguration";

            let err = self
                .client
                .register_capability(vec![Registration {
                    id: CONFIG_REGISTRATION_ID.to_owned(),
                    method: CONFIG_METHOD_ID.to_owned(),
                    register_options: None,
                }])
                .err();
            if let Some(err) = err {
                error!("could not register to watch config changes: {err}");
            }
        }

        self.primary.initialized(params);
        info!("server initialized");
    }

    /// Enters main loop after initialization.
    fn main_loop(&mut self, inbox: crossbeam_channel::Receiver<Message>) -> anyhow::Result<()> {
        // todo: follow what rust analyzer does
        // Windows scheduler implements priority boosts: if thread waits for an
        // event (like a condvar), and event fires, priority of the thread is
        // temporary bumped. This optimization backfires in our case: each time
        // the `main_loop` schedules a task to run on a threadpool, the
        // worker threads gets a higher priority, and (on a machine with
        // fewer cores) displaces the main loop! We work around this by
        // marking the main loop as a higher-priority thread.
        //
        // https://docs.microsoft.com/en-us/windows/win32/procthread/scheduling-priorities
        // https://docs.microsoft.com/en-us/windows/win32/procthread/priority-boosts
        // https://github.com/rust-lang/rust-analyzer/issues/2835
        // #[cfg(windows)]
        // unsafe {
        //     use winapi::um::processthreadsapi::*;
        //     let thread = GetCurrentThread();
        //     let thread_priority_above_normal = 1;
        //     SetThreadPriority(thread, thread_priority_above_normal);
        // }

        while let Some(event) = self.next_event(&inbox) {
            if matches!(
                &event,
                Event::Lsp(lsp_server::Message::Notification(Notification { method, .. }))
                if method == lsp_types::notification::Exit::METHOD
            ) {
                return Ok(());
            }
            self.handle_event(event)?;
        }

        warn!("client exited without proper shutdown sequence");
        Ok(())
    }
}

impl TypstLanguageServer {
    /// Receives the next event from event sources.
    fn next_event(&self, inbox: &Receiver<lsp_server::Message>) -> Option<Event> {
        select! {
            recv(inbox) -> msg =>
                msg.ok().map(Event::Lsp),
        }
    }

    /// Handles an incoming event.
    fn handle_event(&mut self, event: Event) -> anyhow::Result<()> {
        let loop_start = Instant::now();

        // let was_quiescent = self.is_quiescent();
        match event {
            Event::Lsp(msg) => match msg {
                lsp_server::Message::Request(req) => self.on_new_request(loop_start, req),
                lsp_server::Message::Notification(not) => self.on_notification(loop_start, not)?,
                lsp_server::Message::Response(resp) => {
                    self.client.clone().complete_request(self, resp)
                }
            },
        }
        Ok(())
    }

    /// Registers and handles a request. This should only be called once per
    /// incoming request.
    fn on_new_request(&mut self, request_received: Instant, req: Request) {
        self.client.register_request(&req, request_received);
        self.on_request(req);
    }

    /// Handles a request.
    fn on_request(&mut self, req: Request) {
        if self.shutdown_requested {
            self.client.respond(lsp_server::Response::new_err(
                req.id.clone(),
                lsp_server::ErrorCode::InvalidRequest as i32,
                "Shutdown already requested.".to_owned(),
            ));
            return;
        }

        let Some(handler) = self.regular_cmds.get(req.method.as_str()) else {
            warn!("unhandled request: {}", req.method);
            return;
        };

        let res = handler(self, (req.id.clone(), req.params));
        if matches!(res, Ok(Some(()))) {
            return;
        }

        if let Ok(response) = result_to_response_(req.id, res) {
            self.client.respond(response);
        }
    }

    /// Handles an incoming notification.
    fn on_notification(
        &mut self,
        request_received: Instant,
        not: Notification,
    ) -> anyhow::Result<()> {
        info!("notifying {} - at {:0.2?}", not.method, request_received);

        let Some(handler) = self.notify_cmds.get(not.method.as_str()) else {
            warn!("unhandled notification: {}", not.method);
            return Ok(());
        };

        let result = handler(self, not.params);

        let request_duration = request_received.elapsed();
        if let Err(err) = result {
            error!(
                "notifing {} failed in {:0.2?}: {:?}",
                not.method, request_duration, err
            );
        } else {
            info!(
                "notifing {} succeeded in {:0.2?}",
                not.method, request_duration
            );
        }

        Ok(())
    }

    /// Registers or unregisters semantic tokens.
    fn enable_sema_token_caps(&mut self, enable: bool) -> anyhow::Result<()> {
        if !self.const_config().sema_tokens_dynamic_registration {
            trace!("skip register semantic by config");
            return Ok(());
        }

        let res = match (enable, self.sema_tokens_registered) {
            (true, None | Some(false)) => {
                trace!("registering semantic tokens");
                let options = get_semantic_tokens_options();
                self.client
                    .register_capability(vec![get_semantic_tokens_registration(options)])
                    .context("could not register semantic tokens")
            }
            (false, Some(true)) => {
                trace!("unregistering semantic tokens");
                self.client
                    .unregister_capability(vec![get_semantic_tokens_unregistration()])
                    .context("could not unregister semantic tokens")
            }
            (true, Some(true)) | (false, None | Some(false)) => Ok(()),
        };

        if res.is_ok() {
            self.sema_tokens_registered = Some(enable);
        }

        res
    }

    /// Registers or unregisters document formatter.
    fn enable_formatter_caps(&mut self, enable: bool) -> anyhow::Result<()> {
        if !self.const_config().doc_fmt_dynamic_registration {
            trace!("skip dynamic register formatter by config");
            return Ok(());
        }

        const FORMATTING_REGISTRATION_ID: &str = "formatting";
        const DOCUMENT_FORMATTING_METHOD_ID: &str = "textDocument/formatting";

        pub fn get_formatting_registration() -> Registration {
            Registration {
                id: FORMATTING_REGISTRATION_ID.to_owned(),
                method: DOCUMENT_FORMATTING_METHOD_ID.to_owned(),
                register_options: None,
            }
        }

        pub fn get_formatting_unregistration() -> Unregistration {
            Unregistration {
                id: FORMATTING_REGISTRATION_ID.to_owned(),
                method: DOCUMENT_FORMATTING_METHOD_ID.to_owned(),
            }
        }

        let res = match (enable, self.formatter_registered) {
            (true, None | Some(false)) => {
                trace!("registering formatter");
                self.client
                    .register_capability(vec![get_formatting_registration()])
                    .context("could not register formatter")
            }
            (false, Some(true)) => {
                trace!("unregistering formatter");
                self.client
                    .unregister_capability(vec![get_formatting_unregistration()])
                    .context("could not unregister formatter")
            }
            (true, Some(true)) | (false, None | Some(false)) => Ok(()),
        };

        if res.is_ok() {
            self.formatter_registered = Some(enable);
        }

        res
    }
}

/// Trait implemented by language server backends.
///
/// This interface allows servers adhering to the [Language Server Protocol] to
/// be implemented in a safe and easily testable way without exposing the
/// low-level implementation details.
///
/// [Language Server Protocol]: https://microsoft.github.io/language-server-protocol/
impl TypstLanguageServer {
    /// The [`shutdown`] request asks the server to gracefully shut down, but to
    /// not exit.
    ///
    /// [`shutdown`]: https://microsoft.github.io/language-server-protocol/specification#shutdown
    ///
    /// This request is often later followed by an [`exit`] notification, which
    /// will cause the server to exit immediately.
    ///
    /// [`exit`]: https://microsoft.github.io/language-server-protocol/specification#exit
    ///
    /// This method is guaranteed to only execute once. If the client sends this
    /// request to the server again, the server will respond with JSON-RPC
    /// error code `-32600` (invalid request).
    fn shutdown(&mut self, _params: ()) -> LspResult<()> {
        self.shutdown_requested = true;
        Ok(())
    }
}

/// Here are implemented the handlers for each command.
impl TypstLanguageServer {
    fn get_exec_commands() -> ExecuteCmdMap {
        macro_rules! redirected_command {
            ($key: expr, Self::$method: ident) => {
                (
                    $key,
                    exec_fn!(LspHandler<Vec<JsonValue>, JsonValue>, Self::$method, inputs),
                )
            };
        }

        ExecuteCmdMap::from_iter([
            redirected_command!("tinymist.exportPdf", Self::export_pdf),
            redirected_command!("tinymist.exportSvg", Self::export_svg),
            redirected_command!("tinymist.exportPng", Self::export_png),
            redirected_command!("tinymist.doClearCache", Self::clear_cache),
            redirected_command!("tinymist.pinMain", Self::pin_document),
            redirected_command!("tinymist.focusMain", Self::focus_document),
            redirected_command!("tinymist.doInitTemplate", Self::init_template),
            redirected_command!("tinymist.doGetTemplateEntry", Self::do_get_template_entry),
            redirected_command!("tinymist.getDocumentMetrics", Self::get_document_metrics),
            redirected_command!("tinymist.getServerInfo", Self::get_server_info),
            // For Documentations
            redirected_command!("tinymist.getResources", Self::get_resources),
        ])
    }

    /// The entry point for the `workspace/executeCommand` request.
    fn execute_command(&mut self, params: ExecuteCommandParams) -> LspResult<Option<JsonValue>> {
        let ExecuteCommandParams {
            command,
            arguments,
            work_done_progress_params: _,
        } = params;
        let Some(handler) = self.exec_cmds.get(command.as_str()) else {
            error!("asked to execute unknown command");
            return Err(method_not_found());
        };

        Ok(Some(handler(self, arguments)?))
    }

    /// Export the current document as a PDF file.
    pub fn export_pdf(&self, arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        self.export(ExportKind::Pdf, arguments)
    }

    /// Export the current document as a Svg file.
    pub fn export_svg(&self, arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        let opts = parse_opts(arguments.get(1))?;
        self.export(ExportKind::Svg { page: opts.page }, arguments)
    }

    /// Export the current document as a Png file.
    pub fn export_png(&self, arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        let opts = parse_opts(arguments.get(1))?;
        self.export(ExportKind::Png { page: opts.page }, arguments)
    }

    /// Export the current document as some format. The client is responsible
    /// for passing the correct absolute path of typst document.
    pub fn export(&self, kind: ExportKind, arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        let path = parse_path(arguments.first())?.as_ref().to_owned();

        let res = run_query!(self.OnExport(path, kind))?;
        let res = serde_json::to_value(res).map_err(|_| internal_error("Cannot serialize path"))?;

        Ok(res)
    }

    /// Get the metrics of the document.
    pub fn get_document_metrics(&self, arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        let path = parse_path(arguments.first())?.as_ref().to_owned();

        let res = run_query!(self.DocumentMetrics(path))?;
        let res = serde_json::to_value(res)
            .map_err(|e| internal_error(format!("Cannot serialize response {e}")))?;

        Ok(res)
    }

    /// Get the server info.
    pub fn get_server_info(&self, _arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        let res = run_query!(self.ServerInfo())?;

        let res = serde_json::to_value(res)
            .map_err(|e| internal_error(format!("Cannot serialize response {e}")))?;

        Ok(res)
    }

    /// Clear all cached resources.
    ///
    /// # Errors
    /// Errors if the cache could not be cleared.
    pub fn clear_cache(&self, _arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        comemo::evict(0);
        for v in Some(self.primary())
            .into_iter()
            .chain(self.dedicates.iter().map(|v| v.compiler()))
        {
            v.clear_cache();
        }
        Ok(JsonValue::Null)
    }

    /// Pin main file to some path.
    pub fn pin_document(&mut self, arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        let new_entry = parse_path_or_null(arguments.first())?;

        let update_result = self.pin_entry(new_entry.clone());
        update_result.map_err(|err| internal_error(format!("could not pin file: {err}")))?;

        info!("file pinned: {entry:?}", entry = new_entry);
        Ok(JsonValue::Null)
    }

    /// Focus main file to some path.
    pub fn focus_document(&mut self, arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        let new_entry = parse_path_or_null(arguments.first())?;

        let update_result = self.focus_entry(new_entry.clone());
        update_result.map_err(|err| internal_error(format!("could not focus file: {err}")))?;

        info!("file focused: {entry:?}", entry = new_entry);
        Ok(JsonValue::Null)
    }

    /// Initialize a new template.
    pub fn init_template(&self, arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        use crate::tools::package::{self, determine_latest_version, TemplateSource};

        #[derive(Debug, Serialize)]
        struct InitResult {
            #[serde(rename = "entryPath")]
            entry_path: PathBuf,
        }

        let from_source = arguments
            .first()
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned())
            .ok_or_else(|| invalid_params("The first parameter is not a valid source or null"))?;
        let to_path = parse_path_or_null(arguments.get(1))?;
        let res = self
            .primary()
            .steal(move |c| {
                // Parse the package specification. If the user didn't specify the version,
                // we try to figure it out automatically by downloading the package index
                // or searching the disk.
                let spec: PackageSpec = from_source
                    .parse()
                    .or_else(|err| {
                        // Try to parse without version, but prefer the error message of the
                        // normal package spec parsing if it fails.
                        let spec: VersionlessPackageSpec = from_source.parse().map_err(|_| err)?;
                        let version = determine_latest_version(c.compiler.world(), &spec)?;
                        StrResult::Ok(spec.at(version))
                    })
                    .map_err(map_string_err("failed to parse package spec"))?;

                let from_source = TemplateSource::Package(spec);

                let entry_path = package::init(
                    c.compiler.world(),
                    InitTask {
                        tmpl: from_source.clone(),
                        dir: to_path.clone(),
                    },
                )
                .map_err(map_string_err("failed to initialize template"))?;

                info!("template initialized: {from_source:?} to {to_path:?}");

                ZResult::Ok(InitResult { entry_path })
            })
            .and_then(|e| e)
            .map_err(|e| invalid_params(format!("failed to determine template source: {e}")))?;

        serde_json::to_value(res).map_err(|_| internal_error("Cannot serialize path"))
    }

    /// Get the entry of a template.
    pub fn do_get_template_entry(&self, arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        use crate::tools::package::{self, determine_latest_version, TemplateSource};

        let from_source = arguments
            .first()
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned())
            .ok_or_else(|| invalid_params("The first parameter is not a valid source or null"))?;

        let entry = self
            .primary()
            .steal(move |c| {
                // Parse the package specification. If the user didn't specify the version,
                // we try to figure it out automatically by downloading the package index
                // or searching the disk.
                let spec: PackageSpec = from_source
                    .parse()
                    .or_else(|err| {
                        // Try to parse without version, but prefer the error message of the
                        // normal package spec parsing if it fails.
                        let spec: VersionlessPackageSpec = from_source.parse().map_err(|_| err)?;
                        let version = determine_latest_version(c.compiler.world(), &spec)?;
                        StrResult::Ok(spec.at(version))
                    })
                    .map_err(map_string_err("failed to parse package spec"))?;

                let from_source = TemplateSource::Package(spec);

                let entry = package::get_entry(c.compiler.world(), from_source)
                    .map_err(map_string_err("failed to get template entry"))?;

                ZResult::Ok(entry)
            })
            .and_then(|e| e)
            .map_err(|e| invalid_params(format!("failed to determine template entry: {e}")))?;

        let entry = String::from_utf8(entry.to_vec())
            .map_err(|_| invalid_params("template entry is not a valid UTF-8 string"))?;

        Ok(JsonValue::String(entry))
    }
}

impl TypstLanguageServer {
    fn get_resources_routes() -> ResourceMap {
        macro_rules! resources_at {
            ($key: expr, Self::$method: ident) => {
                (
                    Path::new($key).clean().as_path().into(),
                    exec_fn!(LspHandler<Vec<JsonValue>, JsonValue>, Self::$method, inputs),
                )
            };
        }

        ResourceMap::from_iter([
            resources_at!("/symbols", Self::resources_alt_symbols),
            resources_at!("/tutorial", Self::resource_tutoral),
        ])
    }

    /// Get static resources with help of tinymist service, for example, a
    /// static help pages for some typst function.
    pub fn get_resources(&mut self, arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        let u = parse_path(arguments.first())?;

        let Some(handler) = self.resources_routes.get(u.as_ref()) else {
            error!("asked for unknown resource: {u:?}");
            return Err(method_not_found());
        };

        // Note our redirection will keep the first path argument in the arguments vec.
        handler(self, arguments)
    }
    /// Get the all valid symbols
    pub fn resources_alt_symbols(&self, _arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        let resp = self.get_symbol_resources();
        resp.map_err(|e| internal_error(e.to_string()))
    }

    /// Get tutorial web page
    pub fn resource_tutoral(&self, _arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        Err(method_not_found())
    }
}

/// Document Synchronization
impl TypstLanguageServer {
    fn did_open(&mut self, params: DidOpenTextDocumentParams) -> LspResult<()> {
        log::info!("did open {:?}", params.text_document.uri);
        let path = as_path_(params.text_document.uri);
        let text = params.text_document.text;

        self.create_source(path.clone(), text).unwrap();
        Ok(())
    }

    fn did_close(&mut self, params: DidCloseTextDocumentParams) -> LspResult<()> {
        let path = as_path_(params.text_document.uri);

        self.remove_source(path.clone()).unwrap();
        Ok(())
    }

    fn did_change(&mut self, params: DidChangeTextDocumentParams) -> LspResult<()> {
        let path = as_path_(params.text_document.uri);
        let changes = params.content_changes;

        self.edit_source(path.clone(), changes, self.const_config().position_encoding)
            .unwrap();
        Ok(())
    }

    fn did_save(&self, params: DidSaveTextDocumentParams) -> LspResult<()> {
        let path = as_path(params.text_document);

        let _ = run_query!(self.OnSaveExport(path));
        Ok(())
    }

    fn on_changed_configuration(&mut self, values: Map<String, JsonValue>) -> LspResult<()> {
        let config = self.config.clone();
        match self.config.update_by_map(&values) {
            Ok(()) => {}
            Err(err) => {
                self.config = config;
                error!("error applying new settings: {err}");
                return Err(invalid_params(format!(
                    "error applying new settings: {err}"
                )));
            }
        }
        self.primary.on_changed_configuration(values)?;

        info!("new settings applied");

        if config.semantic_tokens != self.config.semantic_tokens {
            let err = self
                .enable_sema_token_caps(self.config.semantic_tokens == SemanticTokensMode::Enable);
            if let Err(err) = err {
                error!("could not change semantic tokens config: {err}");
            }
        }

        if config.formatter != self.config.formatter {
            let err = self.enable_formatter_caps(self.config.formatter != FormatterMode::Disable);
            if let Err(err) = err {
                error!("could not change formatter config: {err}");
            }
            if let Some(f) = &self.format_thread {
                let err = f.send(FormattingRequest::ChangeConfig(FormattingConfig {
                    mode: self.config.formatter,
                    width: 120,
                }));
                if let Err(err) = err {
                    error!("could not change formatter config: {err}");
                }
            }
        }

        // todo: watch changes of the root path

        Ok(())
    }

    fn did_change_configuration(&mut self, params: DidChangeConfigurationParams) -> LspResult<()> {
        // For some clients, we don't get the actual changed configuration and need to
        // poll for it https://github.com/microsoft/language-server-protocol/issues/676
        match params.settings {
            JsonValue::Object(settings) => self.on_changed_configuration(settings)?,
            _ => {
                self.client.send_request::<WorkspaceConfiguration>(
                    ConfigurationParams {
                        items: Config::get_items(),
                    },
                    |this, resp| {
                        if let Some(err) = resp.error {
                            log::error!("failed to request configuration: {err:?}");
                            return;
                        }
                        // .map(Config::values_to_map),

                        let Some(result) = resp.result else {
                            log::error!("no configuration returned");
                            return;
                        };

                        let resp: Vec<JsonValue> = serde_json::from_value(result).unwrap();
                        let _ = this.on_changed_configuration(Config::values_to_map(resp));
                    },
                );
            }
        };

        Ok(())
    }
}

/// Standard Language Features
impl TypstLanguageServer {
    fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(self.GotoDefinition(path, position))
    }

    fn goto_declaration(
        &self,
        params: GotoDeclarationParams,
    ) -> LspResult<Option<GotoDeclarationResponse>> {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(self.GotoDeclaration(path, position))
    }

    fn references(&self, params: ReferenceParams) -> LspResult<Option<Vec<Location>>> {
        let (path, position) = as_path_pos(params.text_document_position);
        run_query!(self.References(path, position))
    }

    fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(self.Hover(path, position))
    }

    fn folding_range(&self, params: FoldingRangeParams) -> LspResult<Option<Vec<FoldingRange>>> {
        let path = as_path(params.text_document);
        let line_folding_only = self.const_config().doc_line_folding_only;
        run_query!(self.FoldingRange(path, line_folding_only))
    }

    fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> LspResult<Option<Vec<SelectionRange>>> {
        let path = as_path(params.text_document);
        let positions = params.positions;
        run_query!(self.SelectionRange(path, positions))
    }

    fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> LspResult<Option<DocumentSymbolResponse>> {
        let path = as_path(params.text_document);
        run_query!(self.DocumentSymbol(path))
    }

    fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        let path = as_path(params.text_document);
        run_query!(self.SemanticTokensFull(path))
    }

    fn semantic_tokens_full_delta(
        &self,
        params: SemanticTokensDeltaParams,
    ) -> LspResult<Option<SemanticTokensFullDeltaResult>> {
        let path = as_path(params.text_document);
        let previous_result_id = params.previous_result_id;
        run_query!(self.SemanticTokensDelta(path, previous_result_id))
    }

    fn formatting(
        &self,
        req_id: RequestId,
        params: DocumentFormattingParams,
    ) -> LspResult<Option<()>> {
        if matches!(self.config.formatter, FormatterMode::Disable) {
            return Ok(None);
        }

        let path = as_path(params.text_document).as_path().into();
        self.query_source(path, |source| {
            if let Some(f) = &self.format_thread {
                f.send(FormattingRequest::Formatting((req_id, source.clone())))?;
            }

            Ok(Some(()))
        })
        .map_err(|e| internal_error(format!("could not format document: {e}")))
    }

    fn inlay_hint(&self, params: InlayHintParams) -> LspResult<Option<Vec<InlayHint>>> {
        let path = as_path(params.text_document);
        let range = params.range;
        run_query!(self.InlayHint(path, range))
    }

    fn code_lens(&self, params: CodeLensParams) -> LspResult<Option<Vec<CodeLens>>> {
        let path = as_path(params.text_document);
        run_query!(self.CodeLens(path))
    }

    fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let (path, position) = as_path_pos(params.text_document_position);
        let explicit = params
            .context
            .map(|context| context.trigger_kind == CompletionTriggerKind::INVOKED)
            .unwrap_or(false);

        run_query!(self.Completion(path, position, explicit))
    }

    fn signature_help(&self, params: SignatureHelpParams) -> LspResult<Option<SignatureHelp>> {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(self.SignatureHelp(path, position))
    }

    fn rename(&self, params: RenameParams) -> LspResult<Option<WorkspaceEdit>> {
        let (path, position) = as_path_pos(params.text_document_position);
        let new_name = params.new_name;
        run_query!(self.Rename(path, position, new_name))
    }

    fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> LspResult<Option<PrepareRenameResponse>> {
        let (path, position) = as_path_pos(params);
        run_query!(self.PrepareRename(path, position))
    }

    fn symbol(&self, params: WorkspaceSymbolParams) -> LspResult<Option<Vec<SymbolInformation>>> {
        let pattern = (!params.query.is_empty()).then_some(params.query);
        run_query!(self.Symbol(pattern))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportOpts {
    page: PageSelection,
}

fn parse_opts(v: Option<&JsonValue>) -> LspResult<ExportOpts> {
    Ok(match v {
        Some(opts) => serde_json::from_value::<ExportOpts>(opts.clone())
            .map_err(|_| invalid_params("The third argument is not a valid object"))?,
        _ => ExportOpts {
            page: PageSelection::First,
        },
    })
}

fn parse_path(v: Option<&JsonValue>) -> LspResult<ImmutPath> {
    let new_entry = match v {
        Some(JsonValue::String(s)) => Path::new(s).clean().as_path().into(),
        _ => return Err(invalid_params("The first parameter is not a valid path")),
    };

    Ok(new_entry)
}

fn parse_path_or_null(v: Option<&JsonValue>) -> LspResult<Option<ImmutPath>> {
    match v {
        Some(JsonValue::Null) => Ok(None),
        v => Ok(Some(parse_path(v)?)),
    }
}

pub fn invalid_params(msg: impl Into<String>) -> ResponseError {
    ResponseError {
        code: ErrorCode::InvalidParams as i32,
        message: msg.into(),
        data: None,
    }
}

pub fn internal_error(msg: impl Into<String>) -> ResponseError {
    ResponseError {
        code: ErrorCode::InternalError as i32,
        message: msg.into(),
        data: None,
    }
}

pub fn method_not_found() -> ResponseError {
    ResponseError {
        code: ErrorCode::MethodNotFound as i32,
        message: "Method not found".to_string(),
        data: None,
    }
}

pub(crate) fn result_to_response_<T: Serialize>(
    id: lsp_server::RequestId,
    result: Result<T, ResponseError>,
) -> Result<lsp_server::Response, Cancelled> {
    let res = match result {
        Ok(resp) => {
            let resp = serde_json::to_value(resp);
            match resp {
                Ok(resp) => lsp_server::Response::new_ok(id, resp),
                Err(e) => return result_to_response(id, Err(internal_error(e.to_string()))),
            }
        }
        Err(e) => lsp_server::Response::new_err(id, e.code, e.message),
    };
    Ok(res)
}

fn result_to_response(
    id: lsp_server::RequestId,
    result: Result<JsonValue, ResponseError>,
) -> Result<lsp_server::Response, Cancelled> {
    let res = match result {
        Ok(resp) => lsp_server::Response::new_ok(id, resp),
        Err(e) => lsp_server::Response::new_err(id, e.code, e.message),
    };
    Ok(res)
}

#[test]
fn test_as_path() {
    let uri = Url::parse("untitled:/path/to/file").unwrap();
    assert_eq!(as_path_(uri), Path::new("/untitled/path/to/file").clean());

    let uri = Url::parse("untitled:/path/to/file%20with%20space").unwrap();
    assert_eq!(
        as_path_(uri),
        Path::new("/untitled/path/to/file with space").clean()
    );
}
