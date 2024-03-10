//! # tinymist
//!
//! This crate provides an integrated service for [Typst](https://typst.app/) [taɪpst]. It provides:
//! + A language server following the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/).
//!
//! ## Architecture
//!
//! Tinymist binary has multiple modes, and it may runs multiple actors in
//! background. The actors could run as an async task, in a single thread, or in
//! an isolated process.
//!
//! The main process of tinymist runs the program as a language server, through
//! stdin and stdout. A main process will fork:
//! - rendering actors to provide PDF export with watching.
//! - compiler actors to provide language APIs.
//!
//! ## Debugging with input mirroring
//!
//! You can record the input during running the editors with Tinymist. You can
//! then replay the input to debug the language server.
//!
//! ```sh
//! # Record the input
//! tinymist --mirror input.txt
//! # Replay the input
//! tinymist --replay input.txt
//! ```

// pub mod formatting;
mod actor;
pub mod transport;

use core::fmt;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use std::{collections::HashMap, path::PathBuf};

use anyhow::{bail, Context};
use crossbeam_channel::select;
use crossbeam_channel::Receiver;
use itertools::Itertools;
use log::{debug, error, info, trace, warn};
use lsp_server::{ErrorCode, Message, Notification, Request, ResponseError};
use lsp_types::notification::{Notification as NotificationTrait, PublishDiagnostics};
use lsp_types::request::{RegisterCapability, UnregisterCapability, WorkspaceConfiguration};
use lsp_types::*;
use once_cell::sync::OnceCell;
use parking_lot::{Mutex, RwLock};
use paste::paste;
use serde::Deserialize;
use serde_json::{Map, Value as JsonValue};
use tinymist_query::{
    get_semantic_tokens_options, get_semantic_tokens_registration,
    get_semantic_tokens_unregistration, PositionEncoding,
};

use crate::actor::typst::CompileCluster;

// Enforces drop order
pub struct Handle<H, C> {
    pub handle: H,
    pub receiver: C,
}

pub type ReqHandler = for<'a> fn(&'a mut TypstLanguageServer, lsp_server::Response);
type ReqQueue = lsp_server::ReqQueue<(String, Instant), ReqHandler>;

/// The host for the language server, or known as the LSP client.
#[derive(Debug, Clone)]
pub struct LspHost {
    sender: crossbeam_channel::Sender<Message>,
    req_queue: Arc<Mutex<ReqQueue>>,
}

impl LspHost {
    /// Creates a new language server host.
    pub fn new(sender: crossbeam_channel::Sender<Message>) -> Self {
        Self {
            sender,
            req_queue: Arc::new(Mutex::new(ReqQueue::default())),
        }
    }

    pub fn send_request<R: lsp_types::request::Request>(
        &self,
        params: R::Params,
        handler: ReqHandler,
    ) {
        let mut req_queue = self.req_queue.lock();
        let request = req_queue
            .outgoing
            .register(R::METHOD.to_owned(), params, handler);
        let Err(res) = self.sender.send(request.into()) else {
            return;
        };
        warn!("failed to send request: {res:?}");
    }

    pub fn complete_request(
        &self,
        service: &mut TypstLanguageServer,
        response: lsp_server::Response,
    ) {
        let mut req_queue = self.req_queue.lock();
        let Some(handler) = req_queue.outgoing.complete(response.id.clone()) else {
            warn!("received response for unknown request");
            return;
        };
        drop(req_queue);
        handler(service, response)
    }

    pub fn send_notification<N: lsp_types::notification::Notification>(&self, params: N::Params) {
        let not = lsp_server::Notification::new(N::METHOD.to_owned(), params);
        let Err(res) = self.sender.send(not.into()) else {
            return;
        };
        warn!("failed to send notification: {res:?}");
    }

    pub fn register_request(&self, request: &lsp_server::Request, request_received: Instant) {
        let mut req_queue = self.req_queue.lock();
        debug!(
            "handling {} - ({}) at {:0.2?}",
            request.method, request.id, request_received
        );
        req_queue.incoming.register(
            request.id.clone(),
            (request.method.clone(), request_received),
        );
    }
    pub fn respond(&self, response: lsp_server::Response) {
        let mut req_queue = self.req_queue.lock();
        if let Some((method, start)) = req_queue.incoming.complete(response.id.clone()) {
            // if let Some(err) = &response.error {
            //     if err.message.starts_with("server panicked") {
            //         self.poke_rust_analyzer_developer(format!("{}, check the log",
            // err.message))     }
            // }

            let duration = start.elapsed();
            debug!(
                "handled  {} - ({}) in {:0.2?}",
                method, response.id, duration
            );
            let Err(res) = self.sender.send(response.into()) else {
                return;
            };
            warn!("failed to send response: {res:?}");
        }
    }

    fn publish_diagnostics(&self, uri: Url, diagnostics: Vec<Diagnostic>, version: Option<i32>) {
        self.send_notification::<PublishDiagnostics>(PublishDiagnosticsParams {
            uri,
            diagnostics,
            version,
        });
    }

    // todo: handle error
    fn register_capability(&self, registrations: Vec<Registration>) -> anyhow::Result<()> {
        self.send_request::<RegisterCapability>(RegistrationParams { registrations }, |_, resp| {
            if let Some(err) = resp.error {
                log::error!("failed to register capability: {err:?}");
            }
        });
        Ok(())
    }

    fn unregister_capability(&self, unregisterations: Vec<Unregistration>) -> anyhow::Result<()> {
        self.send_request::<UnregisterCapability>(
            UnregistrationParams { unregisterations },
            |_, resp| {
                if let Some(err) = resp.error {
                    log::error!("failed to unregister capability: {err:?}");
                }
            },
        );
        Ok(())
    }
}

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

struct Cancelled;

type LspResult<Res> = Result<Res, ResponseError>;
type LspMethod<Res> = fn(srv: &mut TypstLanguageServer, args: JsonValue) -> LspResult<Res>;
type LspHandler<Req, Res> = fn(srv: &mut TypstLanguageServer, args: Req) -> LspResult<Res>;

type ExecuteCmdMap = HashMap<&'static str, LspHandler<Vec<JsonValue>, ()>>;
type NotifyCmdMap = HashMap<&'static str, LspMethod<()>>;
type RegularCmdMap = HashMap<&'static str, LspMethod<JsonValue>>;

macro_rules! exec_fn {
    ($ty: ty, Self::$method: ident, $($arg_key:ident),+ $(,)?) => {{
        const E: $ty = |this, $($arg_key),+| this.$method($($arg_key),+);
        E
    }};
}

macro_rules! request_fn {
    ($desc: ty, Self::$method: ident) => {
        (<$desc>::METHOD, {
            const E: LspMethod<JsonValue> = |this, req| {
                let req: <$desc as lsp_types::request::Request>::Params =
                    serde_json::from_value(req).unwrap(); // todo: soft unwrap
                let res = this.$method(req)?;
                let res = serde_json::to_value(res).unwrap(); // todo: soft unwrap
                Ok(res)
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

macro_rules! run_query {
    ($self: ident.$query: ident ($($arg_key:ident),+ $(,)?)) => {{
        use tinymist_query::*;
        let req = paste! { [<$query Request>] { $($arg_key),+ } };
        $self
            .universe()
            .query(CompilerQueryRequest::$query(req.clone()))
            .map_err(|err| {
                error!("error getting $query: {err} with request {req:?}");
                internal_error()
            })
            .map(|resp| {
                let CompilerQueryResponse::$query(resp) = resp else {
                    unreachable!()
                };
                resp
            })
    }};
}

fn as_path(inp: TextDocumentIdentifier) -> PathBuf {
    inp.uri.to_file_path().unwrap()
}

fn as_path_pos(inp: TextDocumentPositionParams) -> (PathBuf, Position) {
    (as_path(inp.text_document), inp.position)
}

/// The object providing the language server functionality.
pub struct TypstLanguageServer {
    /// The language server client.
    pub client: LspHost,
    /// Whether the server is shutting down.
    pub shutdown_requested: bool,
    /// Extra commands provided with `textDocument/executeCommand`.
    pub exec_cmds: ExecuteCmdMap,
    /// Regular notifications for dispatching.
    pub notify_cmds: NotifyCmdMap,
    /// Regular commands for dispatching.
    pub regular_cmds: RegularCmdMap,
    /// User configuration from the editor.
    pub config: Arc<RwLock<Config>>,
    /// Const configuration initialized at the start of the session.
    /// For example, the position encoding.
    pub const_config: OnceCell<ConstConfig>,
    /// The compiler cluster.
    pub universe: OnceCell<CompileCluster>,
}

/// Getters and the main loop.
impl TypstLanguageServer {
    /// Create a new language server.
    pub fn new(client: LspHost) -> Self {
        Self {
            client,
            shutdown_requested: false,
            config: Default::default(),
            const_config: Default::default(),
            universe: Default::default(),
            exec_cmds: Self::get_exec_commands(),
            regular_cmds: Self::get_regular_cmds(),
            notify_cmds: Self::get_notify_cmds(),
        }
    }

    /// Get the const configuration.
    ///
    /// # Panics
    /// Panics if the const configuration is not initialized.
    pub fn const_config(&self) -> &ConstConfig {
        self.const_config
            .get()
            .expect("const config should be initialized")
    }

    /// Get the compiler cluster.
    ///
    /// # Panics
    /// Panics if the universe is not initialized.
    pub fn universe(&self) -> &CompileCluster {
        self.universe.get().expect("universe should be initialized")
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
            request_fn!(InlayHintRequest, Self::inlay_hint),
            // Sync for low latency
            request_fn!(SelectionRangeRequest, Self::selection_range),
            // latency insensitive
            request_fn!(HoverRequest, Self::hover),
            request_fn!(FoldingRangeRequest, Self::folding_range),
            request_fn!(SignatureHelpRequest, Self::signature_help),
            request_fn!(PrepareRenameRequest, Self::prepare_rename),
            request_fn!(Rename, Self::rename),
            request_fn!(GotoDefinition, Self::goto_definition),
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

    pub fn main_loop(&mut self, inbox: Receiver<Message>) -> anyhow::Result<()> {
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

    fn next_event(&self, inbox: &Receiver<lsp_server::Message>) -> Option<Event> {
        select! {
            recv(inbox) -> msg =>
                msg.ok().map(Event::Lsp),
        }
    }

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

        let result = handler(self, req.params);

        if let Ok(response) = result_to_response(req.id, result) {
            self.client.respond(response);
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
    }

    /// Handles an incoming notification.
    fn on_notification(
        &mut self,
        request_received: Instant,
        not: Notification,
    ) -> anyhow::Result<()> {
        debug!("notifying {} - at {:0.2?}", not.method, request_received);

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
}

/// Trait implemented by language server backends.
///
/// This interface allows servers adhering to the [Language Server Protocol] to
/// be implemented in a safe and easily testable way without exposing the
/// low-level implementation details.
///
/// [Language Server Protocol]: https://microsoft.github.io/language-server-protocol/
impl TypstLanguageServer {
    /// The [`initialize`] request is the first request sent from the client to
    /// the server.
    ///
    /// [`initialize`]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// This method is guaranteed to only execute once. If the client sends this
    /// request to the server again, the server will respond with JSON-RPC
    /// error code `-32600` (invalid request).
    ///
    /// # Panics
    /// Panics if the const configuration is already initialized.
    /// Panics if the cluster is already initialized.
    ///
    /// # Errors
    /// Errors if the configuration could not be updated.
    pub fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        // self.tracing_init();

        // Initialize configurations
        let cc = &self.const_config;
        cc.set(ConstConfig::from(&params))
            .expect("const config is initialized");
        let cc = cc.get().expect("const config is not initialized").clone();

        if let Some(init) = &params.initialization_options {
            let mut config = self.config.write();
            config
                .update(init)
                .as_ref()
                .map_err(ToString::to_string)
                .map_err(invalid_params)?;
        }

        // Bootstrap actors
        let actor_factory = actor::ActorFactory::new(cc);

        // Bootstrap the cluster
        let cluster = actor_factory.prepare_cluster(self.client.clone(), params.root_paths());
        let (cluster, cluster_bg) = cluster.split();
        self.universe
            .set(cluster)
            .map_err(|_| ())
            .expect("the cluster is already initialized");
        // Run the cluster in the background after we referencing it
        tokio::spawn(cluster_bg.run());

        // Respond to the host (LSP client)

        let config = self.config.read();

        let semantic_tokens_provider = match config.semantic_tokens {
            SemanticTokensMode::Enable
                if !params.supports_semantic_tokens_dynamic_registration() =>
            {
                Some(get_semantic_tokens_options().into())
            }
            _ => None,
        };

        let document_formatting_provider = match config.formatter {
            ExperimentalFormatterMode::Enable
                if !params.supports_document_formatting_dynamic_registration() =>
            {
                Some(OneOf::Left(true))
            }
            _ => None,
        };

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                }),
                definition_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        String::from("#"),
                        String::from("."),
                        String::from("@"),
                    ]),
                    ..Default::default()
                }),
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                        ..Default::default()
                    },
                )),
                semantic_tokens_provider,
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: self.exec_cmds.keys().map(ToString::to_string).collect(),
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                }),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                })),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    ..Default::default()
                }),
                document_formatting_provider,
                inlay_hint_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    /// The [`initialized`] notification is sent from the client to the server
    /// after the client received the result of the initialize request but
    /// before the client sends anything else.
    ///
    /// [`initialized`]: https://microsoft.github.io/language-server-protocol/specification#initialized
    ///
    /// The server can use the `initialized` notification, for example, to
    /// dynamically register capabilities with the client.
    pub fn initialized(&self, _: InitializedParams) {
        let const_config = self.const_config();
        let mut config = self.config.write();

        if const_config.supports_semantic_tokens_dynamic_registration {
            trace!("setting up to dynamically register semantic token support");

            let client = self.client.clone();
            let register = move || {
                trace!("dynamically registering semantic tokens");
                let client = client.clone();
                let options = get_semantic_tokens_options();
                client
                    .register_capability(vec![get_semantic_tokens_registration(options)])
                    .context("could not register semantic tokens")
            };

            let client = self.client.clone();
            let unregister = move || {
                trace!("unregistering semantic tokens");
                let client = client.clone();
                client
                    .unregister_capability(vec![get_semantic_tokens_unregistration()])
                    .context("could not unregister semantic tokens")
            };

            if config.semantic_tokens == SemanticTokensMode::Enable {
                if let Some(err) = register().err() {
                    error!("could not dynamically register semantic tokens: {err}");
                }
            }

            config.listen_semantic_tokens(Box::new(move |mode| match mode {
                SemanticTokensMode::Enable => register(),
                SemanticTokensMode::Disable => unregister(),
            }));
        }

        if const_config.supports_config_change_registration {
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

        info!("server initialized");
    }

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
                    exec_fn!(LspHandler<Vec<JsonValue>, ()>, Self::$method, inputs),
                )
            };
        }

        ExecuteCmdMap::from_iter([
            redirected_command!("tinymist.doPdfExport", Self::export_pdf),
            redirected_command!("tinymist.doClearCache", Self::clear_cache),
            redirected_command!("tinymist.doPinMain", Self::pin_main),
            redirected_command!("tinymist.doActivateDoc", Self::activate_doc),
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

        handler(self, arguments)?;
        Ok(Some(JsonValue::Null))
    }

    /// Export the current document as a PDF file. The client is responsible for
    /// passing the correct file URI.
    ///
    /// # Errors
    /// Errors if a provided file URI is not a valid file URI.
    pub fn export_pdf(&self, arguments: Vec<JsonValue>) -> LspResult<()> {
        if arguments.is_empty() {
            return Err(invalid_params("Missing file URI argument"));
        }
        let Some(file_uri) = arguments.first().and_then(|v| v.as_str()) else {
            return Err(invalid_params("Missing file URI as first argument"));
        };
        let file_uri =
            Url::parse(file_uri).map_err(|_| invalid_params("Parameter is not a valid URI"))?;
        let path = file_uri
            .to_file_path()
            .map_err(|_| invalid_params("URI is not a file URI"))?;

        let _ = run_query!(self.OnSaveExport(path));

        Ok(())
    }

    /// Clear all cached resources.
    ///
    /// # Errors
    /// Errors if the cache could not be cleared.
    pub fn clear_cache(&self, _arguments: Vec<JsonValue>) -> LspResult<()> {
        comemo::evict(0);
        Ok(())
    }

    /// Pin main file to some path.
    ///
    /// # Errors
    /// Errors if a provided file URI is not a valid file URI.
    pub fn pin_main(&self, arguments: Vec<JsonValue>) -> LspResult<()> {
        let Some(file_uri) = arguments.first().and_then(|v| v.as_str()) else {
            return Err(invalid_params("Missing file path as the first argument"));
        };

        let file_uri = if file_uri == "detached" {
            None
        } else {
            Some(Url::parse(file_uri).map_err(|_| invalid_params("Parameter is not a valid URI"))?)
        };

        let update_result = self.universe().pin_main(file_uri.clone());

        update_result.map_err(|err| {
            error!("could not set main file: {err}");
            internal_error()
        })?;

        info!("main file pinned: {main_url:?}", main_url = file_uri);
        Ok(())
    }

    /// Change the actived document.
    ///
    /// # Errors
    /// Errors if a provided file URI is not a valid path string.
    /// Errors if the document could not be activated.
    pub fn activate_doc(&self, arguments: Vec<JsonValue>) -> LspResult<()> {
        let Some(path) = arguments.first() else {
            return Err(invalid_params("Missing path argument"));
        };
        let path = match path {
            JsonValue::String(s) => Some(Path::new(s).into()),
            JsonValue::Null => None,
            _ => return Err(invalid_params("Path Parameter is not a string or null")),
        };

        let update_result = self.universe().activate_doc(path.clone());

        update_result.map_err(|err| {
            error!("could not set active document: {err}");
            internal_error()
        })?;

        info!("active document set: {path:?}", path = path);
        Ok(())
    }
}

/// Document Synchronization
impl TypstLanguageServer {
    fn did_open(&self, params: DidOpenTextDocumentParams) -> LspResult<()> {
        let path = params.text_document.uri.to_file_path().unwrap();
        let text = params.text_document.text;

        let universe = self.universe();
        universe.create_source(path.clone(), text).unwrap();
        Ok(())
    }

    fn did_close(&self, params: DidCloseTextDocumentParams) -> LspResult<()> {
        let path = params.text_document.uri.to_file_path().unwrap();

        let universe = self.universe();
        universe.remove_source(path.clone()).unwrap();
        // self.client.publish_diagnostics(uri, Vec::new(), None);
        Ok(())
    }

    fn did_change(&self, params: DidChangeTextDocumentParams) -> LspResult<()> {
        let path = params.text_document.uri.to_file_path().unwrap();
        let changes = params.content_changes;

        let universe = self.universe();
        universe
            .edit_source(path.clone(), changes, self.const_config().position_encoding)
            .unwrap();
        Ok(())
    }

    fn did_save(&self, params: DidSaveTextDocumentParams) -> LspResult<()> {
        let uri = params.text_document.uri;
        let path = uri.to_file_path().unwrap();

        let config = self.config.read();

        if config.export_pdf == ExportPdfMode::OnSave {
            let _ = run_query!(self.OnSaveExport(path));
        }
        Ok(())
    }

    fn on_changed_configuration(&self, values: Map<String, JsonValue>) -> LspResult<()> {
        let mut config = self.config.write();

        match config.update_by_map(&values) {
            Ok(()) => {
                info!("new settings applied");
            }
            Err(err) => {
                error!("error applying new settings: {err}");
            }
        }

        Ok(())
    }

    fn did_change_configuration(&self, params: DidChangeConfigurationParams) -> LspResult<()> {
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

                        let resp = serde_json::from_value(result).unwrap();
                        let _ = this.on_changed_configuration(resp);
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

    fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(self.Hover(path, position))
    }

    fn folding_range(&self, params: FoldingRangeParams) -> LspResult<Option<Vec<FoldingRange>>> {
        let path = as_path(params.text_document);
        let line_folding_only = self.const_config().line_folding_only;
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

    fn inlay_hint(&self, params: InlayHintParams) -> LspResult<Option<Vec<InlayHint>>> {
        let path = as_path(params.text_document);
        let range = params.range;
        run_query!(self.InlayHint(path, range))
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

/// The mode of the experimental formatter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExperimentalFormatterMode {
    /// Disable the experimental formatter.
    #[default]
    Disable,
    /// Enable the experimental formatter.
    Enable,
}

/// The mode of PDF export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportPdfMode {
    /// Don't export PDF automatically.
    Never,
    /// Export PDF on saving the document, i.e. on `textDocument/didSave`
    /// events.
    #[default]
    OnSave,
    /// Export PDF on typing, i.e. on `textDocument/didChange` events.
    OnType,
}

/// The mode of semantic tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticTokensMode {
    /// Disable the semantic tokens.
    Disable,
    /// Enable the semantic tokens.
    #[default]
    Enable,
}

type Listener<T> = Box<dyn FnMut(&T) -> anyhow::Result<()>>;

const CONFIG_ITEMS: &[&str] = &[
    "exportPdf",
    "rootPath",
    "semanticTokens",
    "experimentalFormatterMode",
];

/// The user configuration read from the editor.
#[derive(Default)]
pub struct Config {
    /// The mode of PDF export.
    pub export_pdf: ExportPdfMode,
    /// Specifies the root path of the project manually.
    pub root_path: Option<PathBuf>,
    /// Dynamic configuration for semantic tokens.
    pub semantic_tokens: SemanticTokensMode,
    /// Dynamic configuration for the experimental formatter.
    pub formatter: ExperimentalFormatterMode,
    semantic_tokens_listeners: Vec<Listener<SemanticTokensMode>>,
    formatter_listeners: Vec<Listener<ExperimentalFormatterMode>>,
}

impl Config {
    /// Gets items for serialization.
    pub fn get_items() -> Vec<ConfigurationItem> {
        let sections = CONFIG_ITEMS
            .iter()
            .flat_map(|item| [format!("tinymist.{item}"), item.to_string()]);

        sections
            .map(|section| ConfigurationItem {
                section: Some(section),
                ..Default::default()
            })
            .collect()
    }

    /// Converts values to a map.
    pub fn values_to_map(values: Vec<JsonValue>) -> Map<String, JsonValue> {
        let unpaired_values = values
            .into_iter()
            .tuples()
            .map(|(a, b)| if !a.is_null() { a } else { b });

        CONFIG_ITEMS
            .iter()
            .map(|item| item.to_string())
            .zip(unpaired_values)
            .collect()
    }

    /// Updates the configuration with a JSON object.
    ///
    /// # Errors
    /// Errors if the update is invalid.
    pub fn update(&mut self, update: &JsonValue) -> anyhow::Result<()> {
        if let JsonValue::Object(update) = update {
            self.update_by_map(update)
        } else {
            bail!("got invalid configuration object {update}")
        }
    }

    /// Updates the configuration with a map.
    ///
    /// # Errors
    /// Errors if the update is invalid.
    pub fn update_by_map(&mut self, update: &Map<String, JsonValue>) -> anyhow::Result<()> {
        let export_pdf = update
            .get("exportPdf")
            .map(ExportPdfMode::deserialize)
            .and_then(Result::ok);
        if let Some(export_pdf) = export_pdf {
            self.export_pdf = export_pdf;
        }

        let root_path = update.get("rootPath");
        if let Some(root_path) = root_path {
            if root_path.is_null() {
                self.root_path = None;
            }
            if let Some(root_path) = root_path.as_str().map(PathBuf::from) {
                self.root_path = Some(root_path);
            }
        }

        let semantic_tokens = update
            .get("semanticTokens")
            .map(SemanticTokensMode::deserialize)
            .and_then(Result::ok);
        if let Some(semantic_tokens) = semantic_tokens {
            for listener in &mut self.semantic_tokens_listeners {
                listener(&semantic_tokens)?;
            }
            self.semantic_tokens = semantic_tokens;
        }

        let formatter = update
            .get("experimentalFormatterMode")
            .map(ExperimentalFormatterMode::deserialize)
            .and_then(Result::ok);
        if let Some(formatter) = formatter {
            for listener in &mut self.formatter_listeners {
                listener(&formatter)?;
            }
            self.formatter = formatter;
        }

        Ok(())
    }

    fn listen_semantic_tokens(&mut self, listener: Listener<SemanticTokensMode>) {
        self.semantic_tokens_listeners.push(listener);
    }

    // pub fn listen_formatting(&mut self, listener:
    // Listener<ExperimentalFormatterMode>) {     self.formatter_listeners.
    // push(listener); }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Config")
            .field("export_pdf", &self.export_pdf)
            .field("formatter", &self.formatter)
            .field("semantic_tokens", &self.semantic_tokens)
            .field(
                "semantic_tokens_listeners",
                &format_args!("Vec[len = {}]", self.semantic_tokens_listeners.len()),
            )
            .field(
                "formatter_listeners",
                &format_args!("Vec[len = {}]", self.formatter_listeners.len()),
            )
            .finish()
    }
}

/// Configuration set at initialization that won't change within a single
/// session
#[derive(Debug, Clone)]
pub struct ConstConfig {
    /// The position encoding, either UTF-8 or UTF-16.
    /// Defaults to UTF-16 if not specified.
    pub position_encoding: PositionEncoding,
    /// Whether the client supports dynamic registration of semantic tokens.
    pub supports_semantic_tokens_dynamic_registration: bool,
    /// Whether the client supports dynamic registration of document formatting.
    pub supports_document_formatting_dynamic_registration: bool,
    /// Whether the client supports dynamic registration of configuration
    /// changes.
    pub supports_config_change_registration: bool,
    /// Whether the client only supports line folding.
    pub line_folding_only: bool,
}

impl ConstConfig {
    fn choose_encoding(params: &InitializeParams) -> PositionEncoding {
        let encodings = params.position_encodings();
        if encodings.contains(&PositionEncodingKind::UTF8) {
            PositionEncoding::Utf8
        } else {
            PositionEncoding::Utf16
        }
    }
}

impl From<&InitializeParams> for ConstConfig {
    fn from(params: &InitializeParams) -> Self {
        Self {
            position_encoding: Self::choose_encoding(params),
            supports_semantic_tokens_dynamic_registration: params
                .supports_semantic_tokens_dynamic_registration(),
            supports_document_formatting_dynamic_registration: params
                .supports_document_formatting_dynamic_registration(),
            supports_config_change_registration: params.supports_config_change_registration(),
            line_folding_only: params.line_folding_only(),
        }
    }
}

trait InitializeParamsExt {
    fn position_encodings(&self) -> &[PositionEncodingKind];
    fn supports_config_change_registration(&self) -> bool;
    fn semantic_tokens_capabilities(&self) -> Option<&SemanticTokensClientCapabilities>;
    fn document_formatting_capabilities(&self) -> Option<&DocumentFormattingClientCapabilities>;
    fn supports_semantic_tokens_dynamic_registration(&self) -> bool;
    fn supports_document_formatting_dynamic_registration(&self) -> bool;
    fn line_folding_only(&self) -> bool;
    fn root_paths(&self) -> Vec<PathBuf>;
}

impl InitializeParamsExt for InitializeParams {
    fn position_encodings(&self) -> &[PositionEncodingKind] {
        const DEFAULT_ENCODING: &[PositionEncodingKind; 1] = &[PositionEncodingKind::UTF16];
        self.capabilities
            .general
            .as_ref()
            .and_then(|general| general.position_encodings.as_ref())
            .map(|encodings| encodings.as_slice())
            .unwrap_or(DEFAULT_ENCODING)
    }

    fn supports_config_change_registration(&self) -> bool {
        self.capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.configuration)
            .unwrap_or(false)
    }

    fn line_folding_only(&self) -> bool {
        self.capabilities
            .text_document
            .as_ref()
            .and_then(|workspace| workspace.folding_range.as_ref())
            .and_then(|folding| folding.line_folding_only)
            .unwrap_or(false)
    }

    fn semantic_tokens_capabilities(&self) -> Option<&SemanticTokensClientCapabilities> {
        self.capabilities
            .text_document
            .as_ref()?
            .semantic_tokens
            .as_ref()
    }

    fn document_formatting_capabilities(&self) -> Option<&DocumentFormattingClientCapabilities> {
        self.capabilities
            .text_document
            .as_ref()?
            .formatting
            .as_ref()
    }

    fn supports_semantic_tokens_dynamic_registration(&self) -> bool {
        self.semantic_tokens_capabilities()
            .and_then(|semantic_tokens| semantic_tokens.dynamic_registration)
            .unwrap_or(false)
    }

    fn supports_document_formatting_dynamic_registration(&self) -> bool {
        self.document_formatting_capabilities()
            .and_then(|document_format| document_format.dynamic_registration)
            .unwrap_or(false)
    }

    #[allow(deprecated)] // `self.root_path` is marked as deprecated
    fn root_paths(&self) -> Vec<PathBuf> {
        match self.workspace_folders.as_ref() {
            Some(roots) => roots
                .iter()
                .map(|root| &root.uri)
                .map(Url::to_file_path)
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            None => self
                .root_uri
                .as_ref()
                .map(|uri| uri.to_file_path().unwrap())
                .or_else(|| self.root_path.clone().map(PathBuf::from))
                .into_iter()
                .collect(),
        }
    }
}

fn invalid_params(msg: impl Into<String>) -> ResponseError {
    ResponseError {
        code: ErrorCode::InvalidParams as i32,
        message: msg.into(),
        data: None,
    }
}

fn internal_error() -> ResponseError {
    ResponseError {
        code: ErrorCode::InternalError as i32,
        message: "Internal error".to_string(),
        data: None,
    }
}

fn method_not_found() -> ResponseError {
    ResponseError {
        code: ErrorCode::MethodNotFound as i32,
        message: "Method not found".to_string(),
        data: None,
    }
}
