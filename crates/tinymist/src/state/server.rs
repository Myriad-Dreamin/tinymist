//! tinymist's language server

use std::collections::HashMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use log::{error, info, trace};
use lsp_server::RequestId;
use lsp_types::request::{GotoDeclarationParams, WorkspaceConfiguration};
use lsp_types::*;
use once_cell::sync::OnceCell;
use reflexo_typst::Bytes;
use request::{RegisterCapability, UnregisterCapability};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};
use sync_lsp::*;
use tinymist_project::{EntryResolver, LspCompileSnapshot, ProjectInsId, ProjectResolutionKind};
use tinymist_query::analysis::{Analysis, PeriscopeProvider};
use tinymist_query::{
    to_typst_range, CompilerQueryRequest, CompilerQueryResponse, ExportKind, FoldRequestFeature,
    LocalContext, LspWorldExt, OnExportRequest, PageSelection, PositionEncoding,
    ServerInfoResponse, SyntaxRequest, VersionedDocument,
};
use tinymist_render::PeriscopeRenderer;
use tinymist_std::error::prelude::*;
use tinymist_std::ImmutPath;
use tokio::sync::mpsc;
use typst::layout::Position as TypstPosition;
use typst::{diag::FileResult, syntax::Source};

use crate::actor::editor::EditorActor;
use crate::actor::editor::EditorRequest;
use crate::project::world::EntryState;
use crate::project::LspInterrupt;
use crate::project::{update_lock, PROJECT_ROUTE_USER_ACTION_PRIORITY};
use crate::project::{watch_deps, ProjectPreviewState};
use crate::project::{
    CompileHandlerImpl, ProjectState, QuerySnapFut, QuerySnapWithStat, WorldSnapFut,
};
use crate::project::{CompileServerOpts, ProjectCompiler};
use crate::route::{ProjectResolution, ProjectRouteState};
use crate::stats::CompilerQueryStats;
use crate::task::{
    ExportConfig, ExportTask, ExportUserConfig, FormatTask, FormatterConfig, UserActionTask,
};
use crate::world::vfs::{notify::MemoryEvent, FileChangeSet};
use crate::world::{ImmutDict, LspUniverseBuilder, TaskInputs};
use crate::{init::*, *};

pub(crate) use futures::Future;

pub(crate) fn as_path(inp: TextDocumentIdentifier) -> PathBuf {
    as_path_(inp.uri)
}

pub(crate) fn as_path_(uri: Url) -> PathBuf {
    tinymist_query::url_to_path(uri)
}

fn as_path_pos(inp: TextDocumentPositionParams) -> (PathBuf, Position) {
    (as_path(inp.text_document), inp.position)
}

/// The object providing the language server functionality.
pub struct LanguageState {
    /// The lsp client
    pub client: TypedLspClient<Self>,
    /// The lcok state.
    pub route: ProjectRouteState,
    /// The project state.
    pub project: ProjectState,

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
    /// Source synchronized with client
    pub memory_changes: HashMap<Arc<Path>, MemoryFileMeta>,
    /// The preview state.
    #[cfg(feature = "preview")]
    pub preview: tool::preview::PreviewState,
    /// The diagnostics sender to send diagnostics to `crate::actor::cluster`.
    pub editor_tx: mpsc::UnboundedSender<EditorRequest>,
    /// The formatter tasks running in backend, which will be scheduled by async
    /// runtime.
    pub formatter: FormatTask,
    /// The user action tasks running in backend, which will be scheduled by
    /// async runtime.
    pub user_action: UserActionTask,
}

/// Getters and the main loop.
impl LanguageState {
    /// Create a new language server.
    pub fn new(
        client: TypedLspClient<LanguageState>,
        config: Config,
        editor_tx: mpsc::UnboundedSender<EditorRequest>,
    ) -> Self {
        let formatter = FormatTask::new(config.formatter());

        let default_path = config.compile.entry_resolver.resolve_default();
        let watchers = ProjectPreviewState::default();
        let handle = Self::server(
            &config,
            editor_tx.clone(),
            client.clone(),
            "primary".to_string(),
            config.compile.entry_resolver.resolve(default_path),
            config.compile.determine_inputs(),
            watchers.clone(),
        );

        Self {
            client: client.clone(),
            route: ProjectRouteState::default(),
            project: handle,
            editor_tx,
            memory_changes: HashMap::new(),
            #[cfg(feature = "preview")]
            preview: tool::preview::PreviewState::new(watchers, client.cast(|s| &mut s.preview)),
            ever_focusing_by_activities: false,
            ever_manual_focusing: false,
            sema_tokens_registered: false,
            formatter_registered: false,
            config,

            pinning: false,
            focusing: None,
            formatter,
            user_action: Default::default(),
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

            let err = service.restart_primary();
            if let Err(err) = err {
                error!("could not restart primary: {err}");
            }

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

    /// Install handlers to the language server.
    pub fn install<T: Initializer<S = Self> + AddCommands + 'static>(
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
            // customized event
            .with_event(
                &LspInterrupt::Compile(ProjectInsId::default()),
                State::compile_interrupt::<T>,
            )
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
            .with_request_::<DocumentLinkRequest>(State::document_link)
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
            .with_request_::<WillRenameFiles>(State::will_rename_files)
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
            .with_command("tinymist.exportAnsiHighlight", State::export_ansi_hl)
            .with_command("tinymist.doClearCache", State::clear_cache)
            .with_command("tinymist.pinMain", State::pin_document)
            .with_command("tinymist.focusMain", State::focus_document)
            .with_command("tinymist.doInitTemplate", State::init_template)
            .with_command("tinymist.doGetTemplateEntry", State::get_template_entry)
            .with_command_("tinymist.interactCodeContext", State::interact_code_context)
            .with_command("tinymist.getDocumentTrace", State::get_document_trace)
            .with_command_("tinymist.getDocumentMetrics", State::get_document_metrics)
            .with_command_("tinymist.getWorkspaceLabels", State::get_workspace_labels)
            .with_command_("tinymist.getServerInfo", State::get_server_info)
            // resources
            .with_resource("/fonts", State::resource_fonts)
            .with_resource("/symbols", State::resource_symbols)
            .with_resource("/preview/index.html", State::resource_preview_html)
            .with_resource("/tutorial", State::resource_tutoral)
            .with_resource("/package/by-namespace", State::resource_package_by_ns)
            .with_resource("/package/symbol", State::resource_package_symbols)
            .with_resource("/package/docs", State::resource_package_docs)
            .with_resource("/dir/package", State::resource_package_dirs)
            .with_resource("/dir/package/local", State::resource_local_package_dir);

        // todo: generalize me
        provider.args.add_commands(
            &Some("tinymist.getResources")
                .iter()
                .chain(provider.command_handlers.keys())
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        );

        provider
    }

    fn compile_interrupt<T: Initializer<S = Self>>(
        mut state: ServiceState<T, T::S>,
        params: LspInterrupt,
    ) -> anyhow::Result<()> {
        let _start = std::time::Instant::now();
        // log::info!("incoming interrupt: {params:?}");
        let Some(ready) = state.ready() else {
            log::info!("interrupted on not ready server");
            return Ok(());
        };

        ready.project.interrupt(params);
        // log::info!("interrupted in {:?}", _start.elapsed());
        Ok(())
    }
}

impl LanguageState {
    // todo: handle error
    fn register_capability(&self, registrations: Vec<Registration>) -> Result<()> {
        self.client.send_request_::<RegisterCapability>(
            RegistrationParams { registrations },
            |_, resp| {
                if let Some(err) = resp.error {
                    log::error!("failed to register capability: {err:?}");
                }
            },
        );
        Ok(())
    }

    fn unregister_capability(&self, unregisterations: Vec<Unregistration>) -> Result<()> {
        self.client.send_request_::<UnregisterCapability>(
            UnregistrationParams { unregisterations },
            |_, resp| {
                if let Some(err) = resp.error {
                    log::error!("failed to unregister capability: {err:?}");
                }
            },
        );
        Ok(())
    }

    /// Registers or unregisters semantic tokens.
    fn enable_sema_token_caps(&mut self, enable: bool) -> Result<()> {
        if !self.const_config().tokens_dynamic_registration {
            trace!("skip register semantic by config");
            return Ok(());
        }

        const SEMANTIC_TOKENS_REGISTRATION_ID: &str = "semantic_tokens";
        const SEMANTIC_TOKENS_METHOD_ID: &str = "textDocument/semanticTokens";

        pub fn get_semantic_tokens_registration(options: SemanticTokensOptions) -> Registration {
            Registration {
                id: SEMANTIC_TOKENS_REGISTRATION_ID.to_owned(),
                method: SEMANTIC_TOKENS_METHOD_ID.to_owned(),
                register_options: Some(
                    serde_json::to_value(options)
                        .expect("semantic tokens options should be representable as JSON value"),
                ),
            }
        }

        pub fn get_semantic_tokens_unregistration() -> Unregistration {
            Unregistration {
                id: SEMANTIC_TOKENS_REGISTRATION_ID.to_owned(),
                method: SEMANTIC_TOKENS_METHOD_ID.to_owned(),
            }
        }

        match (enable, self.sema_tokens_registered) {
            (true, false) => {
                trace!("registering semantic tokens");
                let options = get_semantic_tokens_options();
                self.register_capability(vec![get_semantic_tokens_registration(options)])
                    .inspect(|_| self.sema_tokens_registered = enable)
                    .context("could not register semantic tokens")
            }
            (false, true) => {
                trace!("unregistering semantic tokens");
                self.unregister_capability(vec![get_semantic_tokens_unregistration()])
                    .inspect(|_| self.sema_tokens_registered = enable)
                    .context("could not unregister semantic tokens")
            }
            _ => Ok(()),
        }
    }

    /// Registers or unregisters document formatter.
    fn enable_formatter_caps(&mut self, enable: bool) -> Result<()> {
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
                self.register_capability(vec![get_formatting_registration()])
                    .inspect(|_| self.formatter_registered = enable)
                    .context("could not register formatter")
            }
            (false, true) => {
                trace!("unregistering formatter");
                self.unregister_capability(vec![get_formatting_unregistration()])
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

        if config.compile.output_path != self.config.compile.output_path
            || config.compile.export_pdf != self.config.compile.export_pdf
        {
            let config = ExportUserConfig {
                output: self.config.compile.output_path.clone(),
                mode: self.config.compile.export_pdf,
            };

            self.change_export_config(config.clone());
        }

        if config.compile.primary_opts() != self.config.compile.primary_opts() {
            self.config.compile.fonts = OnceCell::new(); // todo: don't reload fonts if not changed
            let err = self.restart_primary();
            if let Err(err) = err {
                error!("could not restart primary: {err}");
            }
        }

        if config.semantic_tokens != self.config.semantic_tokens {
            let err = self
                .enable_sema_token_caps(self.config.semantic_tokens == SemanticTokensMode::Enable);
            if let Err(err) = err {
                error!("could not change semantic tokens config: {err}");
            }
        }

        let new_formatter_config = self.config.formatter();
        if !config.formatter().eq(&new_formatter_config) {
            let enabled = !matches!(new_formatter_config.config, FormatterConfig::Disable);
            let err = self.enable_formatter_caps(enabled);
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

    fn document_link(&mut self, req_id: RequestId, params: DocumentLinkParams) -> ScheduledResult {
        let path = as_path(params.text_document);
        run_query!(req_id, self.DocumentLink(path))
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
        let context = params.context.as_ref();
        let explicit =
            context.is_some_and(|context| context.trigger_kind == CompletionTriggerKind::INVOKED);
        let trigger_character = params
            .context
            .and_then(|c| c.trigger_character)
            .and_then(|c| c.chars().next());

        run_query!(
            req_id,
            self.Completion(path, position, explicit, trigger_character)
        )
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

    fn on_enter(&mut self, req_id: RequestId, params: OnEnterParams) -> ScheduledResult {
        let path = as_path(params.text_document);
        let range = params.range;
        run_query!(req_id, self.OnEnter(path, range))
    }

    fn will_rename_files(
        &mut self,
        req_id: RequestId,
        params: RenameFilesParams,
    ) -> ScheduledResult {
        log::info!("will rename files {params:?}");
        let paths = params
            .files
            .iter()
            .map(|f| {
                Some((
                    as_path_(Url::parse(&f.old_uri).ok()?),
                    as_path_(Url::parse(&f.new_uri).ok()?),
                ))
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| invalid_params("invalid urls"))?;

        run_query!(req_id, self.WillRenameFiles(paths))
    }
}

impl LanguageState {
    /// Focus main file to some path.
    pub fn change_entry(&mut self, path: Option<ImmutPath>) -> Result<bool> {
        if path
            .as_deref()
            .is_some_and(|p| !p.is_absolute() && !p.starts_with("/untitled"))
        {
            return Err(error_once!("entry file must be absolute", path: path.unwrap().display()));
        }

        let next_entry = self.entry_resolver().resolve(path);

        info!("the entry file of TypstActor(primary) is changing to {next_entry:?}");

        self.change_task(TaskInputs {
            entry: Some(next_entry.clone()),
            ..Default::default()
        });

        Ok(true)
    }

    /// Pin the entry to the given path
    pub fn pin_entry(&mut self, new_entry: Option<ImmutPath>) -> Result<()> {
        self.pinning = new_entry.is_some();
        let entry = new_entry
            .or_else(|| self.entry_resolver().resolve_default())
            .or_else(|| self.focusing.clone());
        self.change_entry(entry).map(|_| ())
    }

    /// Updates the primary (focusing) entry
    pub fn focus_entry(&mut self, new_entry: Option<ImmutPath>) -> Result<bool> {
        if self.pinning || self.config.compile.has_default_entry_path {
            self.focusing = new_entry;
            return Ok(false);
        }

        self.change_entry(new_entry.clone())
    }

    /// This is used for tracking activating document status if a client is not
    /// performing any focus command request.
    ///
    /// See <https://github.com/microsoft/language-server-protocol/issues/718>
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

    fn resolve_task(&self, path: ImmutPath) -> TaskInputs {
        let entry = self.entry_resolver().resolve(Some(path));

        TaskInputs {
            entry: Some(entry),
            ..Default::default()
        }
    }

    fn resolve_task_with_state(&mut self, path: ImmutPath) -> TaskInputs {
        let proj_input = matches!(
            self.config.project_resolution,
            ProjectResolutionKind::LockDatabase
        )
        .then(|| {
            let resolution = self.route.resolve(&path)?;
            let lock = self.route.locate(&resolution)?;

            let ProjectResolution {
                lock_dir,
                project_id,
            } = &resolution;

            let input = lock.get_document(project_id)?;
            let root = input
                .root
                .as_ref()
                .and_then(|res| Some(res.to_abs_path(lock_dir)?.as_path().into()))
                .unwrap_or_else(|| lock_dir.clone());
            let main = input
                .main
                .as_ref()
                .and_then(|main| Some(main.to_abs_path(lock_dir)?.as_path().into()))
                .unwrap_or_else(|| path.clone());
            let entry = self
                .entry_resolver()
                .resolve_with_root(Some(root), Some(main));
            log::info!("resolved task with state: {path:?} -> {project_id:?} -> {entry:?}");

            Some(TaskInputs {
                entry: Some(entry),
                ..Default::default()
            })
        });

        proj_input
            .flatten()
            .unwrap_or_else(|| self.resolve_task(path))
    }

    /// Snapshot the compiler thread for tasks
    pub fn snapshot(&mut self) -> Result<WorldSnapFut> {
        self.project.snapshot()
    }

    /// Get the entry resolver.
    pub fn entry_resolver(&self) -> &EntryResolver {
        &self.compile_config().entry_resolver
    }

    /// Snapshot the compiler thread for language queries
    pub fn query_snapshot(&mut self) -> Result<QuerySnapFut> {
        self.project.query_snapshot(None)
    }

    /// Snapshot the compiler thread for language queries
    pub fn query_snapshot_with_stat(
        &mut self,
        q: &CompilerQueryRequest,
    ) -> Result<QuerySnapWithStat> {
        let name: &'static str = q.into();
        let path = q.associated_path();
        let stat = self.project.stats.query_stat(path, name);
        let fut = self.project.query_snapshot(Some(q))?;
        Ok(QuerySnapWithStat { fut, stat })
    }

    fn add_memory_changes(&mut self, event: MemoryEvent) {
        self.project.add_memory_changes(event);
    }

    fn change_task(&mut self, task_inputs: TaskInputs) {
        self.project.change_task(task_inputs);
    }

    pub(crate) fn change_export_config(&mut self, config: ExportUserConfig) {
        self.project.export.change_config(config);
    }

    // pub async fn settle(&mut self) {
    //     let _ = self.change_entry(None);
    //     info!("TypstActor({}): settle requested", self.handle.diag_group);
    //     match self.handle.settle().await {
    //         Ok(()) => info!("TypstActor({}): settled", self.handle.diag_group),
    //         Err(err) => error!(
    //             "TypstActor({}): failed to settle: {err:#}",
    //             self.handle.diag_group
    //         ),
    //     }
    // }

    /// Get the current server info.
    pub fn collect_server_info(&mut self) -> QueryFuture {
        let dg = self.project.diag_group.clone();
        let api_stats = self.project.stats.report();
        let query_stats = self.project.analysis.report_query_stats();
        let alloc_stats = self.project.analysis.report_alloc_stats();

        let snap = self.snapshot()?;
        just_future(async move {
            let snap = snap.receive().await?;
            let w = &snap.world;

            let info = ServerInfoResponse {
                root: w.entry_state().root().map(|e| e.as_ref().to_owned()),
                font_paths: w.font_resolver.font_paths().to_owned(),
                inputs: w.inputs().as_ref().deref().clone(),
                stats: HashMap::from_iter([
                    ("api".to_owned(), api_stats),
                    ("query".to_owned(), query_stats),
                    ("alloc".to_owned(), alloc_stats),
                ]),
            };

            let info = Some(HashMap::from_iter([(dg, info)]));
            Ok(tinymist_query::CompilerQueryResponse::ServerInfo(info))
        })
    }

    /// Export the current document.
    pub fn on_export(&mut self, req: OnExportRequest) -> QueryFuture {
        let OnExportRequest { path, kind, open } = req;
        let entry = self.entry_resolver().resolve(Some(path.as_path().into()));
        let lock_dir = self.compile_config().entry_resolver.resolve_lock(&entry);

        let update_dep = lock_dir.clone().map(|lock_dir| {
            |snap: LspCompileSnapshot| async move {
                let mut updater = update_lock(lock_dir);
                let world = snap.world.clone();
                let doc_id = updater.compiled(&world)?;

                updater.update_materials(doc_id.clone(), snap.world.depended_fs_paths());
                updater.route(doc_id, PROJECT_ROUTE_USER_ACTION_PRIORITY);

                updater.commit();

                Some(())
            }
        });

        let snap = self.snapshot()?;
        let task = self.project.export.factory.task();
        just_future(async move {
            let snap = snap.receive().await?;
            let snap = snap.task(TaskInputs {
                entry: Some(entry),
                ..Default::default()
            });
            let res = task.oneshot(snap.clone(), kind, lock_dir).await?;
            if let Some(update_dep) = update_dep {
                tokio::spawn(update_dep(snap));
            }

            // See https://github.com/Myriad-Dreamin/tinymist/issues/837
            // Also see https://github.com/Byron/open-rs/issues/105
            #[cfg(not(target_os = "windows"))]
            let do_open = ::open::that_detached;
            #[cfg(target_os = "windows")]
            fn do_open(path: impl AsRef<std::ffi::OsStr>) -> std::io::Result<()> {
                ::open::with_detached(path, "explorer")
            }

            if let Some(Some(path)) = open.then_some(res.as_ref()) {
                log::info!("open with system default apps: {path:?}");
                if let Err(e) = do_open(path) {
                    log::error!("failed to open with system default apps: {e}");
                };
            }

            log::info!("CompileActor: on export end: {path:?} as {res:?}");
            Ok(tinymist_query::CompilerQueryResponse::OnExport(res))
        })
    }
}

impl LanguageState {
    /// Restart the primary server.
    pub fn restart_primary(&mut self) -> Result<ProjectInsId> {
        let entry = self.entry_resolver().resolve_default();
        let config = &self.config;

        // todo: hot replacement
        #[cfg(feature = "preview")]
        self.preview.stop_all();

        let new_project = Self::server(
            config,
            self.editor_tx.clone(),
            self.client.clone(),
            "primary".to_string(),
            config.compile.entry_resolver.resolve(entry),
            config.compile.determine_inputs(),
            self.preview.watchers.clone(),
        );

        let mut old_project = std::mem::replace(&mut self.project, new_project);

        rayon::spawn(move || {
            old_project.stop();
        });

        Ok(self.project.state.primary.id.clone())
    }

    /// Restart the server with the given group.
    pub fn restart_dedicate(
        &mut self,
        dedicate: &str,
        entry: Option<ImmutPath>,
    ) -> Result<ProjectInsId> {
        let entry = self.config.compile.entry_resolver.resolve(entry);
        self.project.restart_dedicate(dedicate, entry)
    }

    /// Create a new server for the given group.
    pub fn server(
        config: &Config,
        editor_tx: tokio::sync::mpsc::UnboundedSender<EditorRequest>,
        client: TypedLspClient<LanguageState>,
        diag_group: String,
        entry: EntryState,
        inputs: ImmutDict,
        preview: project::ProjectPreviewState,
    ) -> ProjectState {
        let compile_config = &config.compile;
        let const_config = &config.const_config;

        // use codespan_reporting::term::Config;
        // Run Export actors before preparing cluster to avoid loss of events
        let export_config = ExportConfig {
            group: diag_group.clone(),
            editor_tx: Some(editor_tx.clone()),
            config: ExportUserConfig {
                output: compile_config.output_path.clone(),
                mode: compile_config.export_pdf,
            },
            kind: ExportKind::Pdf {
                creation_timestamp: config.compile.determine_creation_timestamp(),
            },
            count_words: config.compile.notify_status,
        };
        let export = ExportTask::new(client.handle.clone(), export_config);

        log::info!(
            "TypstActor: creating server for {diag_group}, entry: {entry:?}, inputs: {inputs:?}"
        );

        // Create the compile handler for client consuming results.
        let periscope_args = compile_config.periscope_args.clone();
        let handle = Arc::new(CompileHandlerImpl {
            #[cfg(feature = "preview")]
            preview,
            diag_group: diag_group.clone(),
            export: export.clone(),
            editor_tx: editor_tx.clone(),
            client: Box::new(client.clone().to_untyped()),
            analysis: Arc::new(Analysis {
                position_encoding: const_config.position_encoding,
                allow_overlapping_token: const_config.tokens_overlapping_token_support,
                allow_multiline_token: const_config.tokens_multiline_token_support,
                remove_html: !config.support_html_in_markdown,
                completion_feat: config.completion.clone(),
                color_theme: match compile_config.color_theme.as_deref() {
                    Some("dark") => tinymist_query::ColorTheme::Dark,
                    _ => tinymist_query::ColorTheme::Light,
                },
                periscope: periscope_args.map(|args| {
                    let r = TypstPeriscopeProvider(PeriscopeRenderer::new(args));
                    Arc::new(r) as Arc<dyn PeriscopeProvider + Send + Sync>
                }),
                tokens_caches: Arc::default(),
                workers: Default::default(),
                caches: Default::default(),
                analysis_rev_cache: Arc::default(),
                stats: Arc::default(),
            }),

            notified_revision: parking_lot::Mutex::new(0),
        });

        let font_resolver = compile_config.determine_fonts();
        let entry_ = entry.clone();
        let compile_handle = handle.clone();
        let cert_path = compile_config.determine_certification_path();
        let package = compile_config.determine_package_opts();

        // todo: never fail?
        let default_fonts = Arc::new(LspUniverseBuilder::only_embedded_fonts().unwrap());
        let package_registry =
            LspUniverseBuilder::resolve_package(cert_path.clone(), Some(&package));
        let verse =
            LspUniverseBuilder::build(entry_.clone(), inputs, default_fonts, package_registry)
                .expect("incorrect options");

        // todo: unify filesystem watcher
        let (dep_tx, dep_rx) = tokio::sync::mpsc::unbounded_channel();
        let fs_client = client.clone().to_untyped();
        let async_handle = client.handle.clone();
        async_handle.spawn(watch_deps(dep_rx, move |event| {
            fs_client.send_event(LspInterrupt::Fs(event));
        }));

        // Create the actor
        let server = ProjectCompiler::new(
            verse,
            dep_tx,
            CompileServerOpts {
                handler: compile_handle,
                enable_watch: true,
                ..Default::default()
            },
        );
        let font_client = client.clone();
        client.handle.spawn_blocking(move || {
            // Create the world
            let font_resolver = font_resolver.wait().clone();
            font_client.send_event(LspInterrupt::Font(font_resolver));
        });

        // todo: restart loses the memory changes
        // We do send memory changes instead of initializing compiler with them.
        // This is because there are state recorded inside of the compiler actor, and we
        // must update them.
        // client.add_memory_changes(MemoryEvent::Update(snapshot));
        ProjectState {
            diag_group,
            state: server,
            preview: Default::default(),
            analysis: handle.analysis.clone(),
            stats: CompilerQueryStats::default(),
            export: handle.export.clone(),
        }
    }
}

struct TypstPeriscopeProvider(PeriscopeRenderer);

impl PeriscopeProvider for TypstPeriscopeProvider {
    /// Resolve periscope image at the given position.
    fn periscope_at(
        &self,
        ctx: &mut LocalContext,
        doc: VersionedDocument,
        pos: TypstPosition,
    ) -> Option<String> {
        self.0.render_marked(ctx, doc, pos)
    }
}

impl LanguageState {
    fn update_source(&mut self, files: FileChangeSet) -> Result<()> {
        self.add_memory_changes(MemoryEvent::Update(files.clone()));

        Ok(())
    }

    /// Create a new source file.
    pub fn create_source(&mut self, path: PathBuf, content: String) -> Result<()> {
        let path: ImmutPath = path.into();

        log::info!("create source: {path:?}");
        self.memory_changes.insert(
            path.clone(),
            MemoryFileMeta {
                content: Source::detached(content.clone()),
            },
        );

        let content: Bytes = content.as_bytes().into();

        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_inserts(vec![(path, FileResult::Ok(content).into())]);

        self.update_source(files)
    }

    /// Remove a source file.
    pub fn remove_source(&mut self, path: PathBuf) -> Result<()> {
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
    ) -> Result<()> {
        let path: ImmutPath = path.into();

        let meta = self
            .memory_changes
            .get_mut(&path)
            .ok_or_else(|| error_once!("file missing", path: path.display()))?;

        for change in content {
            let replacement = change.text;
            match change.range {
                Some(lsp_range) => {
                    let range = to_typst_range(lsp_range, position_encoding, &meta.content)
                        .expect("invalid range");
                    meta.content.edit(range, &replacement);
                }
                None => {
                    meta.content.replace(&replacement);
                }
            }
        }

        let snapshot = FileResult::Ok(meta.content.text().as_bytes().into()).into();

        let files = FileChangeSet::new_inserts(vec![(path.clone(), snapshot)]);

        self.update_source(files)
    }

    /// Query a source file.
    pub fn query_source<T>(
        &self,
        path: ImmutPath,
        f: impl FnOnce(Source) -> Result<T>,
    ) -> Result<T> {
        let snapshot = self.memory_changes.get(&path);
        let snapshot = snapshot.ok_or_else(|| anyhow::anyhow!("file missing {path:?}"))?;
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

impl LanguageState {
    /// Perform a language query.
    pub fn query(&mut self, query: CompilerQueryRequest) -> QueryFuture {
        use CompilerQueryRequest::*;

        let is_pinning = self.pinning;
        just_ok(match query {
            FoldingRange(req) => query_source!(self, FoldingRange, req)?,
            SelectionRange(req) => query_source!(self, SelectionRange, req)?,
            DocumentSymbol(req) => query_source!(self, DocumentSymbol, req)?,
            OnEnter(req) => query_source!(self, OnEnter, req)?,
            ColorPresentation(req) => CompilerQueryResponse::ColorPresentation(req.request()),
            OnExport(req) => return self.on_export(req),
            ServerInfo(_) => return self.collect_server_info(),
            // todo: query on dedicate projects
            _ => return self.query_on(is_pinning, query),
        })
    }

    fn query_on(&mut self, is_pinning: bool, query: CompilerQueryRequest) -> QueryFuture {
        use CompilerQueryRequest::*;
        type R = CompilerQueryResponse;
        assert!(query.fold_feature() != FoldRequestFeature::ContextFreeUnique);

        let fut_stat = self.query_snapshot_with_stat(&query)?;
        let input = query
            .associated_path()
            .map(|path| self.resolve_task_with_state(path.into()))
            .or_else(|| {
                let root = self.entry_resolver().root(None)?;
                Some(TaskInputs {
                    entry: Some(EntryState::new_rooted_by_id(root, *DETACHED_ENTRY)),
                    ..Default::default()
                })
            });

        just_future(async move {
            let mut snap = fut_stat.fut.receive().await?;
            // todo: whether it is safe to inherit success_doc with changed entry
            if !is_pinning {
                if let Some(input) = input {
                    snap = snap.task(input);
                }
            }
            fut_stat.stat.snap();

            if matches!(query, Completion(..)) {
                // Prefetch the package index for completion.
                if snap.world.registry.cached_index().is_none() {
                    let registry = snap.world.registry.clone();
                    tokio::spawn(async move {
                        let _ = registry.download_index();
                    });
                }
            }

            match query {
                SemanticTokensFull(req) => snap.run_semantic(req, R::SemanticTokensFull),
                SemanticTokensDelta(req) => snap.run_semantic(req, R::SemanticTokensDelta),
                InteractCodeContext(req) => snap.run_semantic(req, R::InteractCodeContext),
                Hover(req) => snap.run_stateful(req, R::Hover),
                GotoDefinition(req) => snap.run_stateful(req, R::GotoDefinition),
                GotoDeclaration(req) => snap.run_semantic(req, R::GotoDeclaration),
                References(req) => snap.run_stateful(req, R::References),
                InlayHint(req) => snap.run_semantic(req, R::InlayHint),
                DocumentHighlight(req) => snap.run_semantic(req, R::DocumentHighlight),
                DocumentColor(req) => snap.run_semantic(req, R::DocumentColor),
                DocumentLink(req) => snap.run_semantic(req, R::DocumentLink),
                CodeAction(req) => snap.run_semantic(req, R::CodeAction),
                CodeLens(req) => snap.run_semantic(req, R::CodeLens),
                Completion(req) => snap.run_stateful(req, R::Completion),
                SignatureHelp(req) => snap.run_semantic(req, R::SignatureHelp),
                Rename(req) => snap.run_stateful(req, R::Rename),
                WillRenameFiles(req) => snap.run_stateful(req, R::WillRenameFiles),
                PrepareRename(req) => snap.run_stateful(req, R::PrepareRename),
                Symbol(req) => snap.run_semantic(req, R::Symbol),
                WorkspaceLabel(req) => snap.run_semantic(req, R::WorkspaceLabel),
                DocumentMetrics(req) => snap.run_stateful(req, R::DocumentMetrics),
                _ => unreachable!(),
            }
        })
    }
}

/// Metadata for a source file.
#[derive(Debug, Clone)]
pub struct MemoryFileMeta {
    /// The content of the file.
    pub content: Source,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportOpts {
    page: PageSelection,
}

/// A parameter for the `experimental/onEnter` command.
///
/// @since 3.17.0
#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnEnterParams {
    /// The text document.
    pub text_document: TextDocumentIdentifier,

    /// The visible document range for which `onEnter` edits should be computed.
    pub range: Range,
}

struct OnEnter;
impl lsp_types::request::Request for OnEnter {
    type Params = OnEnterParams;
    type Result = Option<Vec<TextEdit>>;
    const METHOD: &'static str = "experimental/onEnter";
}

#[test]
fn test_as_path() {
    use reflexo::path::PathClean;
    use std::path::Path;

    let uri = Url::parse("untitled:/path/to/file").unwrap();
    assert_eq!(as_path_(uri), Path::new("/untitled/path/to/file").clean());

    let uri = Url::parse("untitled:/path/to/file%20with%20space").unwrap();
    assert_eq!(
        as_path_(uri),
        Path::new("/untitled/path/to/file with space").clean()
    );
}
