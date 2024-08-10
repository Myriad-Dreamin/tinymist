//! tinymist LSP server

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use actor::editor::EditorActor;
use anyhow::anyhow;
use anyhow::Context;
use log::{error, info, trace};
use lsp_server::RequestId;
use lsp_types::request::{GotoDeclarationParams, WorkspaceConfiguration};
use lsp_types::*;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};
use sync_lsp::*;
use task::{CacheTask, ExportUserConfig, FormatTask, FormatUserConfig, UserActionTask};
use tinymist_query::{
    get_semantic_tokens_options, get_semantic_tokens_registration,
    get_semantic_tokens_unregistration, PageSelection, SemanticTokenContext,
};
use tinymist_query::{
    lsp_to_typst, CompilerQueryRequest, CompilerQueryResponse, FoldRequestFeature, OnExportRequest,
    PositionEncoding, SyntaxRequest,
};
use tokio::sync::mpsc;
use typst::{diag::FileResult, syntax::Source};
use typst_ts_compiler::TaskInputs;
use typst_ts_compiler::{
    vfs::notify::{FileChangeSet, MemoryEvent},
    Time,
};
use typst_ts_core::{error::prelude::*, Bytes, Error, ImmutPath};

use super::{init::*, *};
use crate::actor::editor::EditorRequest;
use crate::actor::typ_client::CompileClientActor;

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
    /// The lsp client
    pub client: TypedLspClient<Self>,

    // State to synchronize with the client.
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

    // Resources
    /// The semantic token context.
    pub tokens_ctx: SemanticTokenContext,
    /// Source synchronized with client
    pub memory_changes: HashMap<Arc<Path>, MemoryFileMeta>,
    /// The preview state.
    #[cfg(feature = "preview")]
    pub preview: tool::preview::PreviewState,
    /// The diagnostics sender to send diagnostics to `crate::actor::cluster`.
    pub editor_tx: mpsc::UnboundedSender<EditorRequest>,
    /// The primary compiler actor.
    pub primary: Option<CompileClientActor>,
    /// The compiler actors for tasks
    pub dedicates: Vec<CompileClientActor>,
    /// The formatter tasks running in backend, which will be scheduled by async
    /// runtime.
    pub formatter: FormatTask,
    /// The user action tasks running in backend, which will be scheduled by
    /// async runtime.
    pub user_action: UserActionTask,
    /// The cache task running in backend
    pub cache: CacheTask,
}

/// Getters and the main loop.
impl LanguageState {
    /// Create a new language server.
    pub fn new(
        client: TypedLspClient<LanguageState>,
        config: Config,
        editor_tx: mpsc::UnboundedSender<EditorRequest>,
    ) -> Self {
        let const_config = &config.const_config;
        let tokens_ctx = SemanticTokenContext::new(
            const_config.position_encoding,
            const_config.tokens_overlapping_token_support,
            const_config.tokens_multiline_token_support,
        );
        let formatter = FormatTask::new(FormatUserConfig {
            mode: config.formatter_mode,
            width: config.formatter_print_width.unwrap_or(120),
            position_encoding: const_config.position_encoding,
        });

        Self {
            client: client.clone(),
            editor_tx,
            primary: None,
            dedicates: Vec::new(),
            memory_changes: HashMap::new(),
            #[cfg(feature = "preview")]
            preview: tool::preview::PreviewState::new(client.cast(|s| &mut s.preview)),
            ever_focusing_by_activities: false,
            ever_manual_focusing: false,
            sema_tokens_registered: false,
            formatter_registered: false,
            config,

            pinning: false,
            focusing: None,
            tokens_ctx,
            formatter,
            user_action: Default::default(),
            cache: CacheTask::default(),
        }
    }

    /// The entry point for the language server.
    pub fn main(client: TypedLspClient<Self>, config: Config, start: bool) -> Self {
        info!("LanguageState: initialized with config {config:?}");

        // Bootstrap server
        let (editor_tx, editor_rx) = mpsc::unbounded_channel();

        let mut service = LanguageState::new(client.clone(), config, editor_tx);

        if start {
            let editor_actor = EditorActor::new(
                client.clone().to_untyped(),
                editor_rx,
                service.config.compile.notify_status,
            );

            service.restart_primary();

            // Run the cluster in the background after we referencing it
            client.handle.spawn(editor_actor.run());
        }

        service
    }

    /// Get the const configuration.
    pub fn const_config(&self) -> &ConstConfig {
        &self.config.const_config
    }

    /// Get the compile configuration.
    pub fn compile_config(&self) -> &CompileConfig {
        &self.config.compile
    }

    /// Get the primary compile server for those commands without task context.
    pub fn primary(&self) -> &CompileClientActor {
        self.primary.as_ref().expect("primary")
    }

    /// Get the task-dedicated compile server.
    pub fn dedicate(&self, group: &str) -> Option<&CompileClientActor> {
        self.dedicates
            .iter()
            .find(|dedicate| dedicate.handle.diag_group == group)
    }

    /// Get all compile servers in current state.
    pub fn servers_mut(&mut self) -> impl Iterator<Item = &mut CompileClientActor> {
        self.primary.iter_mut().chain(self.dedicates.iter_mut())
    }

    /// Install handlers to the language server.
    pub fn install<T: Initializer<S = Self> + AddCommands>(
        provider: LspBuilder<T>,
    ) -> LspBuilder<T> {
        type State = LanguageState;
        use lsp_types::notification::*;
        use lsp_types::request::*;

        #[cfg(feature = "preview")]
        let provider = provider
            .with_command("tinymist.doStartPreview", State::start_preview)
            .with_command("tinymist.doKillPreview", State::kill_preview)
            .with_command("tinymist.scrollPreview", State::scroll_preview);

        // todo: .on_sync_mut::<notifs::Cancel>(handlers::handle_cancel)?
        let mut provider = provider
            .with_request::<Shutdown>(State::shutdown)
            // lantency sensitive
            .with_request_::<Completion>(State::completion)
            .with_request_::<SemanticTokensFullRequest>(State::semantic_tokens_full)
            .with_request_::<SemanticTokensFullDeltaRequest>(State::semantic_tokens_full_delta)
            .with_request_::<DocumentHighlightRequest>(State::document_highlight)
            .with_request_::<DocumentSymbolRequest>(State::document_symbol)
            // Sync for low latency
            .with_request_::<Formatting>(State::formatting)
            .with_request_::<SelectionRangeRequest>(State::selection_range)
            // latency insensitive
            .with_request_::<InlayHintRequest>(State::inlay_hint)
            .with_request_::<DocumentColor>(State::document_color)
            .with_request_::<ColorPresentationRequest>(State::color_presentation)
            .with_request_::<HoverRequest>(State::hover)
            .with_request_::<CodeActionRequest>(State::code_action)
            .with_request_::<CodeLensRequest>(State::code_lens)
            .with_request_::<FoldingRangeRequest>(State::folding_range)
            .with_request_::<SignatureHelpRequest>(State::signature_help)
            .with_request_::<PrepareRenameRequest>(State::prepare_rename)
            .with_request_::<Rename>(State::rename)
            .with_request_::<GotoDefinition>(State::goto_definition)
            .with_request_::<GotoDeclaration>(State::goto_declaration)
            .with_request_::<References>(State::references)
            .with_request_::<WorkspaceSymbolRequest>(State::symbol)
            .with_request_::<OnEnter>(State::on_enter)
            // notifications
            .with_notification::<Initialized>(State::initialized)
            .with_notification::<DidOpenTextDocument>(State::did_open)
            .with_notification::<DidCloseTextDocument>(State::did_close)
            .with_notification::<DidChangeTextDocument>(State::did_change)
            .with_notification::<DidSaveTextDocument>(State::did_save)
            .with_notification::<DidChangeConfiguration>(State::did_change_configuration)
            // commands
            .with_command_("tinymist.exportPdf", State::export_pdf)
            .with_command_("tinymist.exportSvg", State::export_svg)
            .with_command_("tinymist.exportPng", State::export_png)
            .with_command_("tinymist.exportText", State::export_text)
            .with_command_("tinymist.exportHtml", State::export_html)
            .with_command_("tinymist.exportMarkdown", State::export_markdown)
            .with_command_("tinymist.exportQuery", State::export_query)
            .with_command("tinymist.doClearCache", State::clear_cache)
            .with_command("tinymist.pinMain", State::pin_document)
            .with_command("tinymist.focusMain", State::focus_document)
            .with_command("tinymist.doInitTemplate", State::init_template)
            .with_command("tinymist.doGetTemplateEntry", State::get_template_entry)
            .with_command_("tinymist.interactCodeContext", State::interact_code_context)
            .with_command("tinymist.getDocumentTrace", State::get_document_trace)
            .with_command_("tinymist.getDocumentMetrics", State::get_document_metrics)
            .with_command_("tinymist.getServerInfo", State::get_server_info)
            // resources
            .with_resource("/symbols", State::resource_symbols)
            .with_resource("/preview/index.html", State::resource_preview_html)
            .with_resource("/tutorial", State::resource_tutoral);

        // todo: generalize me
        provider.args.add_commands(
            &Some("tinymist.getResources")
                .iter()
                .chain(provider.exec_cmds.keys())
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        );

        provider
    }

    /// Get all sources in current state.
    pub fn vfs_snapshot(&self) -> FileChangeSet {
        FileChangeSet::new_inserts(
            self.memory_changes
                .iter()
                .map(|(path, meta)| {
                    let content = meta.content.clone().text().as_bytes().into();
                    (path.clone(), FileResult::Ok((meta.mt, content)).into())
                })
                .collect(),
        )
    }
}

impl LanguageState {
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
    /// The [`initialized`] notification is sent from the client to the server
    /// after the client received the result of the initialize request but
    /// before the client sends anything else.
    ///
    /// [`initialized`]: https://microsoft.github.io/language-server-protocol/specification#initialized
    ///
    /// The server can use the `initialized` notification, for example, to
    /// dynamically register capabilities with the client.
    fn initialized(&mut self, _params: InitializedParams) -> LspResult<()> {
        if self.const_config().tokens_dynamic_registration
            && self.config.semantic_tokens == SemanticTokensMode::Enable
        {
            let err = self.enable_sema_token_caps(true);
            if let Err(err) = err {
                error!("could not register semantic tokens for initialization: {err}");
            }
        }

        if self.const_config().doc_fmt_dynamic_registration
            && self.config.formatter_mode != FormatterMode::Disable
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

        info!("server initialized");
        Ok(())
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
    fn shutdown(&mut self, _params: ()) -> SchedulableResponse<()> {
        just_ok(())
    }
}

/// Document Synchronization
impl LanguageState {
    fn did_open(&mut self, params: DidOpenTextDocumentParams) -> LspResult<()> {
        log::info!("did open {:?}", params.text_document.uri);
        let path = as_path_(params.text_document.uri);
        let text = params.text_document.text;

        self.create_source(path.clone(), text)
            .map_err(|e| invalid_params(e.to_string()))?;

        // Focus after opening
        self.implicit_focus_entry(|| Some(path.as_path().into()), 'o');
        Ok(())
    }

    fn did_close(&mut self, params: DidCloseTextDocumentParams) -> LspResult<()> {
        let path = as_path_(params.text_document.uri);

        self.remove_source(path.clone())
            .map_err(|e| invalid_params(e.to_string()))?;
        Ok(())
    }

    fn did_change(&mut self, params: DidChangeTextDocumentParams) -> LspResult<()> {
        let path = as_path_(params.text_document.uri);
        let changes = params.content_changes;

        self.edit_source(path.clone(), changes, self.const_config().position_encoding)
            .map_err(|e| invalid_params(e.to_string()))?;
        Ok(())
    }

    fn did_save(&mut self, _params: DidSaveTextDocumentParams) -> LspResult<()> {
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

        for e in self.primary.iter_mut().chain(self.dedicates.iter_mut()) {
            e.sync_config(self.config.compile.clone());
        }

        if config.compile.output_path != self.config.compile.output_path
            || config.compile.export_pdf != self.config.compile.export_pdf
        {
            let config = ExportUserConfig {
                output: self.config.compile.output_path.clone(),
                mode: self.config.compile.export_pdf,
            };

            self.primary
                .as_mut()
                .unwrap()
                .change_export_config(config.clone());
        }

        if config.compile.primary_opts() != self.config.compile.primary_opts() {
            self.config.compile.fonts = OnceCell::new(); // todo: don't reload fonts if not changed
            self.restart_primary();
            // todo: restart dedicates
        }

        if config.semantic_tokens != self.config.semantic_tokens {
            let err = self
                .enable_sema_token_caps(self.config.semantic_tokens == SemanticTokensMode::Enable);
            if let Err(err) = err {
                error!("could not change semantic tokens config: {err}");
            }
        }

        let new_formatter_config = self.config.formatter();
        if config.formatter() != new_formatter_config {
            let err =
                self.enable_formatter_caps(new_formatter_config.mode != FormatterMode::Disable);
            if let Err(err) = err {
                error!("could not change formatter config: {err}");
            }

            self.formatter.change_config(new_formatter_config);
        }

        info!("new settings applied");
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

macro_rules! run_query {
    ($req_id: ident, $self: ident.$query: ident ($($arg_key:ident),* $(,)?)) => {{
        use tinymist_query::*;
        let req = paste::paste! { [<$query Request>] { $($arg_key),* } };
        let query_fut = $self.query(CompilerQueryRequest::$query(req.clone()));
        $self.client.schedule_query($req_id, query_fut)
    }};
}
pub(crate) use run_query;

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

    fn formatting(
        &mut self,
        req_id: RequestId,
        params: DocumentFormattingParams,
    ) -> ScheduledResult {
        if matches!(self.config.formatter_mode, FormatterMode::Disable) {
            return Ok(None);
        }

        let path: ImmutPath = as_path(params.text_document).as_path().into();
        let source = self
            .query_source(path, |source: typst::syntax::Source| Ok(source))
            .map_err(|e| internal_error(format!("could not format document: {e}")))?;
        self.client.schedule(req_id, self.formatter.run(source))
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

impl LanguageState {
    /// Focus main file to some path.
    pub fn do_change_entry(&mut self, new_entry: Option<ImmutPath>) -> Result<bool, Error> {
        self.primary
            .as_mut()
            .unwrap()
            .change_entry(new_entry.clone())
    }

    /// Pin the entry to the given path
    pub fn pin_entry(&mut self, new_entry: Option<ImmutPath>) -> Result<(), Error> {
        self.pinning = new_entry.is_some();
        let entry = new_entry
            .or_else(|| self.config.compile.determine_default_entry_path())
            .or_else(|| self.focusing.clone());
        self.do_change_entry(entry).map(|_| ())
    }

    /// Updates the primary (focusing) entry
    pub fn focus_entry(&mut self, new_entry: Option<ImmutPath>) -> Result<bool, Error> {
        if self.pinning || self.config.compile.has_default_entry_path {
            self.focusing = new_entry;
            return Ok(false);
        }

        self.do_change_entry(new_entry.clone())
    }

    /// This is used for tracking activating document status if a client is not
    /// performing any focus command request.
    ///
    /// See https://github.com/microsoft/language-server-protocol/issues/718
    ///
    /// we do want to focus the file implicitly by `textDocument/diagnostic`
    /// (pullDiagnostics mode), as suggested by language-server-protocol#718,
    /// however, this has poor support, e.g. since neovim 0.10.0.
    pub fn implicit_focus_entry(
        &mut self,
        new_entry: impl FnOnce() -> Option<ImmutPath>,
        site: char,
    ) {
        if self.ever_manual_focusing {
            return;
        }
        // didOpen
        match site {
            // foldingRange, hover, semanticTokens
            'f' | 'h' | 't' => {
                self.ever_focusing_by_activities = true;
            }
            // didOpen
            _ => {
                if self.ever_focusing_by_activities {
                    return;
                }
            }
        }

        let new_entry = new_entry();

        let update_result = self.focus_entry(new_entry.clone());
        match update_result {
            Ok(true) => {
                log::info!("file focused[implicit,{site}]: {new_entry:?}");
            }
            Err(err) => {
                log::warn!("could not focus file: {err}");
            }
            Ok(false) => {}
        }
    }
}

impl LanguageState {
    fn update_source(&mut self, files: FileChangeSet) -> Result<(), Error> {
        for srv in self.servers_mut() {
            srv.add_memory_changes(MemoryEvent::Update(files.clone()));
        }

        Ok(())
    }

    /// Create a new source file.
    pub fn create_source(&mut self, path: PathBuf, content: String) -> Result<(), Error> {
        let now = Time::now();
        let path: ImmutPath = path.into();

        self.memory_changes.insert(
            path.clone(),
            MemoryFileMeta {
                mt: now,
                content: Source::detached(content.clone()),
            },
        );

        let content: Bytes = content.as_bytes().into();
        log::info!("create source: {:?}", path);

        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_inserts(vec![(path, FileResult::Ok((now, content)).into())]);

        self.update_source(files)
    }

    /// Remove a source file.
    pub fn remove_source(&mut self, path: PathBuf) -> Result<(), Error> {
        let path: ImmutPath = path.into();

        self.memory_changes.remove(&path);
        log::info!("remove source: {path:?}");

        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_removes(vec![path]);

        self.update_source(files)
    }

    /// Edit a source file.
    pub fn edit_source(
        &mut self,
        path: PathBuf,
        content: Vec<TextDocumentContentChangeEvent>,
        position_encoding: PositionEncoding,
    ) -> Result<(), Error> {
        let now = Time::now();
        let path: ImmutPath = path.into();

        let meta = self
            .memory_changes
            .get_mut(&path)
            .ok_or_else(|| error_once!("file missing", path: path.display()))?;

        for change in content {
            let replacement = change.text;
            match change.range {
                Some(lsp_range) => {
                    let range = lsp_to_typst::range(lsp_range, position_encoding, &meta.content)
                        .expect("invalid range");
                    meta.content.edit(range, &replacement);
                }
                None => {
                    meta.content.replace(&replacement);
                }
            }
        }

        meta.mt = now;

        let snapshot = FileResult::Ok((now, meta.content.text().as_bytes().into())).into();

        let files = FileChangeSet::new_inserts(vec![(path.clone(), snapshot)]);

        self.update_source(files)
    }

    /// Query a source file.
    pub fn query_source<T>(
        &self,
        path: ImmutPath,
        f: impl FnOnce(Source) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let snapshot = self.memory_changes.get(&path);
        let snapshot = snapshot.ok_or_else(|| anyhow!("file missing {path:?}"))?;
        let source = snapshot.content.clone();
        f(source)
    }
}

macro_rules! query_source {
    ($self:ident, $method:ident, $req:expr) => {{
        let path: ImmutPath = $req.path.clone().into();

        $self.query_source(path, |source| {
            let enc = $self.const_config().position_encoding;
            let res = $req.request(&source, enc);
            Ok(CompilerQueryResponse::$method(res))
        })
    }};
}

macro_rules! query_tokens_cache {
    ($self:ident, $method:ident, $req:expr) => {{
        let path: ImmutPath = $req.path.clone().into();

        $self.query_source(path, |source| {
            let res = $req.request(&$self.tokens_ctx, source);
            Ok(CompilerQueryResponse::$method(res))
        })
    }};
}

impl LanguageState {
    /// Perform a language query.
    pub fn query(&mut self, query: CompilerQueryRequest) -> QueryFuture {
        use CompilerQueryRequest::*;

        let primary = || self.primary();
        let is_pinning = self.pinning;
        just_ok(match query {
            InteractCodeContext(req) => query_source!(self, InteractCodeContext, req)?,
            SemanticTokensFull(req) => query_tokens_cache!(self, SemanticTokensFull, req)?,
            SemanticTokensDelta(req) => query_tokens_cache!(self, SemanticTokensDelta, req)?,
            FoldingRange(req) => query_source!(self, FoldingRange, req)?,
            SelectionRange(req) => query_source!(self, SelectionRange, req)?,
            DocumentSymbol(req) => query_source!(self, DocumentSymbol, req)?,
            OnEnter(req) => query_source!(self, OnEnter, req)?,
            ColorPresentation(req) => CompilerQueryResponse::ColorPresentation(req.request()),
            OnExport(OnExportRequest { kind, path }) => return primary().on_export(kind, path),
            ServerInfo(_) => return primary().collect_server_info(),
            _ => return Self::query_on(primary(), is_pinning, query),
        })
    }

    fn query_on(
        client: &CompileClientActor,
        is_pinning: bool,
        query: CompilerQueryRequest,
    ) -> QueryFuture {
        use CompilerQueryRequest::*;
        type R = CompilerQueryResponse;
        assert!(query.fold_feature() != FoldRequestFeature::ContextFreeUnique);

        let snap = client.snapshot()?;
        let handle = client.handle.clone();
        let entry = query
            .associated_path()
            .map(|path| client.config.determine_entry(Some(path.into())));

        just_future(async move {
            let mut snap = snap.receive().await?;
            // todo: whether it is safe to inherit success_doc with changed entry
            if !is_pinning {
                snap = snap.task(TaskInputs {
                    entry,
                    ..Default::default()
                });
            }

            match query {
                Hover(req) => handle.run_stateful(snap, req, R::Hover),
                GotoDefinition(req) => handle.run_stateful(snap, req, R::GotoDefinition),
                GotoDeclaration(req) => handle.run_semantic(snap, req, R::GotoDeclaration),
                References(req) => handle.run_semantic(snap, req, R::References),
                InlayHint(req) => handle.run_semantic(snap, req, R::InlayHint),
                DocumentHighlight(req) => handle.run_semantic(snap, req, R::DocumentHighlight),
                DocumentColor(req) => handle.run_semantic(snap, req, R::DocumentColor),
                CodeAction(req) => handle.run_semantic(snap, req, R::CodeAction),
                CodeLens(req) => handle.run_semantic(snap, req, R::CodeLens),
                Completion(req) => handle.run_stateful(snap, req, R::Completion),
                SignatureHelp(req) => handle.run_semantic(snap, req, R::SignatureHelp),
                Rename(req) => handle.run_stateful(snap, req, R::Rename),
                PrepareRename(req) => handle.run_stateful(snap, req, R::PrepareRename),
                Symbol(req) => handle.run_semantic(snap, req, R::Symbol),
                DocumentMetrics(req) => handle.run_stateful(snap, req, R::DocumentMetrics),
                _ => unreachable!(),
            }
        })
    }
}

/// Metadata for a source file.
#[derive(Debug, Clone)]
pub struct MemoryFileMeta {
    /// The last modified time.
    pub mt: Time,
    /// The content of the file.
    pub content: Source,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportOpts {
    page: PageSelection,
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
