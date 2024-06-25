//! tinymist LSP mode

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{bail, Context};
use futures::future::BoxFuture;
use log::{error, info, trace, warn};
use lsp_server::{ErrorCode, Message, Notification, Request, RequestId, Response, ResponseError};
use lsp_types::notification::Notification as NotificationTrait;
use lsp_types::request::{GotoDeclarationParams, WorkspaceConfiguration};
use lsp_types::*;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};
use tinymist_query::{
    get_semantic_tokens_options, get_semantic_tokens_registration,
    get_semantic_tokens_unregistration, PageSelection, SemanticTokenContext,
};
use tokio::sync::mpsc;
use typst_ts_core::ImmutPath;

use super::{lsp_init::*, *};
use crate::actor::editor::EditorRequest;
use crate::actor::format::{FormatConfig, FormatRequest};
use crate::actor::typ_client::CompileClientActor;
use crate::actor::user_action::UserActionRequest;
use crate::compile::CompileState;
use crate::compile_init::ConstCompileConfig;
use crate::harness::{InitializedLspDriver, LspHost};
use crate::{run_query, run_query_tail, LspResult};

pub type MaySyncResult<'a> = Result<JsonValue, BoxFuture<'a, JsonValue>>;

pub(super) fn as_path(inp: TextDocumentIdentifier) -> PathBuf {
    as_path_(inp.uri)
}

pub(super) fn as_path_(uri: Url) -> PathBuf {
    tinymist_query::url_to_path(uri)
}

fn as_path_pos(inp: TextDocumentPositionParams) -> (PathBuf, Position) {
    (as_path(inp.text_document), inp.position)
}

/// The object providing the language server functionality.
pub struct LanguageState {
    /// The language server client.
    pub client: LspHost<LanguageState>,

    // State to synchronize with the client.
    /// Whether the server is shutting down.
    pub shutdown_requested: bool,
    /// Whether the server has registered semantic tokens capabilities.
    pub sema_tokens_registered: bool,
    /// Whether the server has registered document formatter capabilities.
    pub formatter_registered: bool,
    /// Whether client is pinning a file.
    pub pinning: bool,
    /// The client focusing file.
    pub focusing: Option<ImmutPath>,
    /// The client ever focused implicitly by activities.
    pub ever_focusing_by_activities: bool,
    /// The client ever sent manual focusing request.
    pub ever_manual_focusing: bool,

    // Configurations
    /// User configuration from the editor.
    pub config: Config,
    /// Const configuration initialized at the start of the session.
    /// For example, the position encoding.
    pub const_config: ConstConfig,

    // Command maps
    /// Extra commands provided with `textDocument/executeCommand`.
    pub exec_cmds: ExecuteCmdMap<Self>,
    /// Regular notifications for dispatching.
    pub notify_cmds: NotifyCmdMap<Self>,
    /// Regular commands for dispatching.
    pub regular_cmds: RegularCmdMap<Self>,
    /// Regular commands for dispatching.
    pub resource_routes: ResourceMap<Self>,

    // Resources
    /// The tokio handle.
    pub handle: tokio::runtime::Handle,
    /// The semantic token context.
    pub tokens_ctx: SemanticTokenContext,
    /// The compiler for general purpose.
    pub primary: CompileState,
    /// The compilers for tasks
    pub dedicates: Vec<CompileState>,
    /// The formatter thread running in backend.
    /// Note: The thread will exit if you drop the sender.
    pub format_thread: Option<crossbeam_channel::Sender<FormatRequest>>,
    /// The user action thread running in backend.
    /// Note: The thread will exit if you drop the sender.
    pub user_action_thread: Option<crossbeam_channel::Sender<UserActionRequest>>,
}

/// Getters and the main loop.
impl LanguageState {
    /// Create a new language server.
    pub fn new(
        client: LspHost<LanguageState>,
        const_config: ConstConfig,
        editor_tx: mpsc::UnboundedSender<EditorRequest>,
        handle: tokio::runtime::Handle,
    ) -> Self {
        let tokens_ctx = SemanticTokenContext::new(
            const_config.position_encoding,
            const_config.tokens_overlapping_token_support,
            const_config.tokens_multiline_token_support,
        );
        Self {
            client: client.clone(),
            primary: CompileState::new(
                LspHost::new(Arc::new(RwLock::new(None))),
                Default::default(),
                ConstCompileConfig {
                    position_encoding: const_config.position_encoding,
                },
                editor_tx,
                handle.clone(),
                client.to_untyped(),
            ),
            handle,
            dedicates: Vec::new(),
            shutdown_requested: false,
            ever_focusing_by_activities: false,
            ever_manual_focusing: false,
            sema_tokens_registered: false,
            formatter_registered: false,
            config: Default::default(),
            const_config,

            exec_cmds: Self::get_exec_commands(),
            regular_cmds: Self::get_regular_cmds(),
            notify_cmds: Self::get_notify_cmds(),
            resource_routes: Self::get_resource_routes(),

            pinning: false,
            focusing: None,
            tokens_ctx,
            format_thread: None,
            user_action_thread: None,
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
    fn get_regular_cmds() -> RegularCmdMap<Self> {
        use lsp_types::request::*;
        RegularCmdMap::from_iter([
            request_fn!(Shutdown, LanguageState::shutdown),
            // lantency sensitive
            request_fn_!(Completion, LanguageState::completion),
            request_fn_!(SemanticTokensFullRequest, LanguageState::semantic_tokens_full),
            request_fn_!(SemanticTokensFullDeltaRequest, LanguageState::semantic_tokens_full_delta),
            request_fn_!(DocumentHighlightRequest, LanguageState::document_highlight),
            request_fn_!(DocumentSymbolRequest, LanguageState::document_symbol),
            // Sync for low latency
            request_fn_!(Formatting, LanguageState::formatting),
            request_fn_!(SelectionRangeRequest, LanguageState::selection_range),
            // latency insensitive
            request_fn_!(InlayHintRequest, LanguageState::inlay_hint),
            request_fn_!(DocumentColor, LanguageState::document_color),
            request_fn_!(ColorPresentationRequest, LanguageState::color_presentation),
            request_fn_!(HoverRequest, LanguageState::hover),
            request_fn_!(CodeActionRequest, LanguageState::code_action),
            request_fn_!(CodeLensRequest, LanguageState::code_lens),
            request_fn_!(FoldingRangeRequest, LanguageState::folding_range),
            request_fn_!(SignatureHelpRequest, LanguageState::signature_help),
            request_fn_!(PrepareRenameRequest, LanguageState::prepare_rename),
            request_fn_!(Rename, LanguageState::rename),
            request_fn_!(GotoDefinition, LanguageState::goto_definition),
            request_fn_!(GotoDeclaration, LanguageState::goto_declaration),
            request_fn_!(References, LanguageState::references),
            request_fn_!(WorkspaceSymbolRequest, LanguageState::symbol),
            request_fn_!(OnEnter, LanguageState::on_enter),
            request_fn_!(ExecuteCommand, LanguageState::on_execute_command),
        ])
    }

    fn get_notify_cmds() -> NotifyCmdMap<Self> {
        // todo: .on_sync_mut::<notifs::Cancel>(handlers::handle_cancel)?
        use lsp_types::notification::*;
        NotifyCmdMap::from_iter([
            notify_fn!(DidOpenTextDocument, LanguageState::did_open),
            notify_fn!(DidCloseTextDocument, LanguageState::did_close),
            notify_fn!(DidChangeTextDocument, LanguageState::did_change),
            notify_fn!(DidSaveTextDocument, LanguageState::did_save),
            notify_fn!(
                DidChangeConfiguration,
                LanguageState::did_change_configuration
            ),
        ])
    }

    pub fn schedule<T: Serialize + 'static>(
        &mut self,
        req_id: RequestId,
        resp: SchedulableResponse<T>,
    ) -> ScheduledResult {
        let resp = resp?;
        let client = self.client.clone();
        self.handle.spawn(async move {
            client.respond(result_to_response(req_id, resp.await));
        });
        Ok(Some(()))
    }
}

impl InitializedLspDriver for LanguageState {
    /// The [`initialized`] notification is sent from the client to the server
    /// after the client received the result of the initialize request but
    /// before the client sends anything else.
    ///
    /// [`initialized`]: https://microsoft.github.io/language-server-protocol/specification#initialized
    ///
    /// The server can use the `initialized` notification, for example, to
    /// dynamically register capabilities with the client.
    fn initialized(&mut self, params: InitializedParams) {
        if self.const_config().tokens_dynamic_registration
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

        while let Ok(msg) = inbox.recv() {
            const EXIT_METHOD: &str = lsp_types::notification::Exit::METHOD;
            let loop_start = Instant::now();
            match msg {
                Message::Notification(not) if not.method == EXIT_METHOD => return Ok(()),
                Message::Notification(not) => self.on_notification(loop_start, not)?,
                Message::Request(req) => self.on_request(loop_start, req),
                Message::Response(resp) => self.client.clone().complete_request(self, resp),
            }
        }

        warn!("client exited without proper shutdown sequence");
        Ok(())
    }
}

impl LanguageState {
    /// Registers and handles a request. This should only be called once per
    /// incoming request.
    fn on_request(&mut self, request_received: Instant, req: Request) {
        self.client.register_request(&req, request_received);

        if self.shutdown_requested {
            self.client.respond(Response::new_err(
                req.id.clone(),
                ErrorCode::InvalidRequest as i32,
                "Shutdown already requested.".to_owned(),
            ));
            return;
        }

        let Some(handler) = self.regular_cmds.get(req.method.as_str()) else {
            warn!("unhandled request: {}", req.method);
            return;
        };

        let result = handler(self, req.id.clone(), req.params);
        match result {
            Ok(Some(())) => {}
            _ => self.client.respond(result_to_response(req.id, result)),
        }
    }

    /// The entry point for the `workspace/executeCommand` request.
    fn on_execute_command(
        &mut self,
        req_id: RequestId,
        params: ExecuteCommandParams,
    ) -> LspResult<Option<()>> {
        let ExecuteCommandParams {
            command, arguments, ..
        } = params;
        let Some(handler) = self.exec_cmds.get(command.as_str()) else {
            error!("asked to execute unknown command");
            return Err(method_not_found());
        };

        handler(self, req_id.clone(), arguments)
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
        if !self.const_config().tokens_dynamic_registration {
            trace!("skip register semantic by config");
            return Ok(());
        }

        match (enable, self.sema_tokens_registered) {
            (true, false) => {
                trace!("registering semantic tokens");
                let options = get_semantic_tokens_options();
                self.client
                    .register_capability(vec![get_semantic_tokens_registration(options)])
                    .inspect(|_| self.sema_tokens_registered = enable)
                    .context("could not register semantic tokens")
            }
            (false, true) => {
                trace!("unregistering semantic tokens");
                self.client
                    .unregister_capability(vec![get_semantic_tokens_unregistration()])
                    .inspect(|_| self.sema_tokens_registered = enable)
                    .context("could not unregister semantic tokens")
            }
            _ => Ok(()),
        }
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

        match (enable, self.formatter_registered) {
            (true, false) => {
                trace!("registering formatter");
                self.client
                    .register_capability(vec![get_formatting_registration()])
                    .inspect(|_| self.formatter_registered = enable)
                    .context("could not register formatter")
            }
            (false, true) => {
                trace!("unregistering formatter");
                self.client
                    .unregister_capability(vec![get_formatting_unregistration()])
                    .inspect(|_| self.formatter_registered = enable)
                    .context("could not unregister formatter")
            }
            _ => Ok(()),
        }
    }
}

/// Trait implemented by language server backends.
///
/// This interface allows servers adhering to the [Language Server Protocol] to
/// be implemented in a safe and easily testable way without exposing the
/// low-level implementation details.
///
/// [Language Server Protocol]: https://microsoft.github.io/language-server-protocol/
impl LanguageState {
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
    fn shutdown(&mut self, _params: ()) -> SchedulableResponse<()> {
        self.shutdown_requested = true;
        just_result!(())
    }
}

/// Document Synchronization
impl LanguageState {
    fn did_open(&mut self, params: DidOpenTextDocumentParams) -> LspResult<()> {
        log::info!("did open {:?}", params.text_document.uri);
        let path = as_path_(params.text_document.uri);
        let text = params.text_document.text;

        self.create_source(path.clone(), text).unwrap();

        // Focus after opening
        self.implicit_focus_entry(|| Some(path.as_path().into()), 'o');
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

    fn did_save(&mut self, params: DidSaveTextDocumentParams) -> LspResult<()> {
        let path = as_path(params.text_document);

        run_query_tail!(self.OnSaveExport(path))?;
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
                let err = f.send(FormatRequest::ChangeConfig(FormatConfig {
                    mode: self.config.formatter,
                    width: self.config.formatter_print_width,
                }));
                if let Err(err) = err {
                    error!("could not change formatter config: {err}");
                }
            }
        }

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
impl LanguageState {
    fn goto_definition(
        &mut self,
        req_id: RequestId,
        params: GotoDefinitionParams,
    ) -> ScheduledResult {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(req_id, self.GotoDefinition(path, position))
    }

    fn goto_declaration(
        &mut self,
        req_id: RequestId,
        params: GotoDeclarationParams,
    ) -> ScheduledResult {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(req_id, self.GotoDeclaration(path, position))
    }

    fn references(&mut self, req_id: RequestId, params: ReferenceParams) -> ScheduledResult {
        let (path, position) = as_path_pos(params.text_document_position);
        run_query!(req_id, self.References(path, position))
    }

    fn hover(&mut self, req_id: RequestId, params: HoverParams) -> ScheduledResult {
        let (path, position) = as_path_pos(params.text_document_position_params);
        self.implicit_focus_entry(|| Some(path.as_path().into()), 'h');
        run_query!(req_id, self.Hover(path, position))
    }

    fn folding_range(&mut self, req_id: RequestId, params: FoldingRangeParams) -> ScheduledResult {
        let path = as_path(params.text_document);
        let line_folding_only = self.const_config().doc_line_folding_only;
        self.implicit_focus_entry(|| Some(path.as_path().into()), 'f');
        run_query!(req_id, self.FoldingRange(path, line_folding_only))
    }

    fn selection_range(
        &mut self,
        req_id: RequestId,
        params: SelectionRangeParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        let positions = params.positions;
        run_query!(req_id, self.SelectionRange(path, positions))
    }

    fn document_highlight(
        &mut self,
        req_id: RequestId,
        params: DocumentHighlightParams,
    ) -> ScheduledResult {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(req_id, self.DocumentHighlight(path, position))
    }

    fn document_symbol(
        &mut self,
        req_id: RequestId,
        params: DocumentSymbolParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        run_query!(req_id, self.DocumentSymbol(path))
    }

    fn semantic_tokens_full(
        &mut self,
        req_id: RequestId,
        params: SemanticTokensParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        self.implicit_focus_entry(|| Some(path.as_path().into()), 't');
        run_query!(req_id, self.SemanticTokensFull(path))
    }

    fn semantic_tokens_full_delta(
        &mut self,
        req_id: RequestId,
        params: SemanticTokensDeltaParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        let previous_result_id = params.previous_result_id;
        self.implicit_focus_entry(|| Some(path.as_path().into()), 't');
        run_query!(req_id, self.SemanticTokensDelta(path, previous_result_id))
    }

    fn formatting(&self, req_id: RequestId, params: DocumentFormattingParams) -> ScheduledResult {
        if matches!(self.config.formatter, FormatterMode::Disable) {
            return Ok(None);
        }

        let path: ImmutPath = as_path(params.text_document).as_path().into();
        self.query_source(path, |source: typst::syntax::Source| {
            if let Some(f) = &self.format_thread {
                f.send(FormatRequest::Format(req_id, source.clone()))?;
            } else {
                bail!("formatter thread is not available");
            }

            Ok(Some(()))
        })
        .map_err(|e| internal_error(format!("could not format document: {e}")))
    }

    fn inlay_hint(&mut self, req_id: RequestId, params: InlayHintParams) -> ScheduledResult {
        let path = as_path(params.text_document);
        let range = params.range;
        run_query!(req_id, self.InlayHint(path, range))
    }

    fn document_color(
        &mut self,
        req_id: RequestId,
        params: DocumentColorParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        run_query!(req_id, self.DocumentColor(path))
    }

    fn color_presentation(
        &mut self,
        req_id: RequestId,
        params: ColorPresentationParams,
    ) -> ScheduledResult {
        let path = as_path(params.text_document);
        let color = params.color;
        let range = params.range;
        run_query!(req_id, self.ColorPresentation(path, color, range))
    }

    fn code_action(&mut self, req_id: RequestId, params: CodeActionParams) -> ScheduledResult {
        let path = as_path(params.text_document);
        let range = params.range;
        run_query!(req_id, self.CodeAction(path, range))
    }

    fn code_lens(&mut self, req_id: RequestId, params: CodeLensParams) -> ScheduledResult {
        let path = as_path(params.text_document);
        run_query!(req_id, self.CodeLens(path))
    }

    fn completion(&mut self, req_id: RequestId, params: CompletionParams) -> ScheduledResult {
        let (path, position) = as_path_pos(params.text_document_position);
        let explicit = params
            .context
            .map(|context| context.trigger_kind == CompletionTriggerKind::INVOKED)
            .unwrap_or(false);

        run_query!(req_id, self.Completion(path, position, explicit))
    }

    fn signature_help(
        &mut self,
        req_id: RequestId,
        params: SignatureHelpParams,
    ) -> ScheduledResult {
        let (path, position) = as_path_pos(params.text_document_position_params);
        run_query!(req_id, self.SignatureHelp(path, position))
    }

    fn rename(&mut self, req_id: RequestId, params: RenameParams) -> ScheduledResult {
        let (path, position) = as_path_pos(params.text_document_position);
        let new_name = params.new_name;
        run_query!(req_id, self.Rename(path, position, new_name))
    }

    fn prepare_rename(
        &mut self,
        req_id: RequestId,
        params: TextDocumentPositionParams,
    ) -> ScheduledResult {
        let (path, position) = as_path_pos(params);
        run_query!(req_id, self.PrepareRename(path, position))
    }

    fn symbol(&mut self, req_id: RequestId, params: WorkspaceSymbolParams) -> ScheduledResult {
        let pattern = (!params.query.is_empty()).then_some(params.query);
        run_query!(req_id, self.Symbol(pattern))
    }

    fn on_enter(
        &mut self,
        req_id: RequestId,
        params: TextDocumentPositionParams,
    ) -> ScheduledResult {
        let (path, position) = as_path_pos(params);
        run_query!(req_id, self.OnEnter(path, position))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportOpts {
    page: PageSelection,
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

pub fn z_internal_error(msg: typst_ts_core::Error) -> ResponseError {
    ResponseError {
        code: ErrorCode::InternalError as i32,
        message: format!("internal: {msg:?}"),
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

pub(crate) fn result_to_response<T: Serialize>(
    id: RequestId,
    result: Result<T, ResponseError>,
) -> Response {
    match result {
        Ok(resp) => match serde_json::to_value(resp) {
            Ok(resp) => Response::new_ok(id, resp),
            Err(e) => {
                let e = internal_error(e.to_string());
                Response::new_err(id, e.code, e.message)
            }
        },
        Err(e) => Response::new_err(id, e.code, e.message),
    }
}

struct OnEnter;
impl lsp_types::request::Request for OnEnter {
    type Params = TextDocumentPositionParams;
    type Result = Option<Vec<TextEdit>>;
    const METHOD: &'static str = "experimental/onEnter";
}

#[test]
fn test_as_path() {
    use std::path::Path;
    use typst_ts_core::path::PathClean;

    let uri = Url::parse("untitled:/path/to/file").unwrap();
    assert_eq!(as_path_(uri), Path::new("/untitled/path/to/file").clean());

    let uri = Url::parse("untitled:/path/to/file%20with%20space").unwrap();
    assert_eq!(
        as_path_(uri),
        Path::new("/untitled/path/to/file with space").clean()
    );
}
