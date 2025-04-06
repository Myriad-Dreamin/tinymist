use std::collections::HashMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lsp_types::request::ShowMessageRequest;
use lsp_types::*;
use reflexo::debug_loc::LspPosition;
use sync_ls::*;
use tinymist_query::{LspWorldExt, OnExportRequest, ServerInfoResponse};
use tinymist_std::error::prelude::*;
use tinymist_std::ImmutPath;
use tinymist_task::ProjectTask;
use tokio::sync::mpsc;
use typst::syntax::Source;

use crate::actor::editor::{EditorActor, EditorRequest};
use crate::lsp::query::OnEnter;
use crate::project::{
    update_lock, CompiledArtifact, EntryResolver, LspComputeGraph, LspInterrupt, ProjectInsId,
    ProjectState, PROJECT_ROUTE_USER_ACTION_PRIORITY,
};
use crate::route::ProjectRouteState;
use crate::task::{ExportTask, FormatTask, UserActionTask};
use crate::world::TaskInputs;
use crate::{lsp::init::*, *};

pub(crate) use futures::Future;

pub(crate) fn as_path(inp: TextDocumentIdentifier) -> PathBuf {
    as_path_(inp.uri)
}

pub(crate) fn as_path_(uri: Url) -> PathBuf {
    tinymist_query::url_to_path(uri)
}

pub(crate) fn as_path_pos(inp: TextDocumentPositionParams) -> (PathBuf, Position) {
    (as_path(inp.text_document), inp.position)
}

/// The object providing the language server functionality.
pub struct ServerState {
    /// The lsp client
    pub client: TypedLspClient<Self>,

    // State
    /// The project route state.
    pub route: ProjectRouteState,
    /// The project state.
    pub project: ProjectState,
    /// The preview state.
    #[cfg(feature = "preview")]
    pub preview: tool::preview::PreviewState,
    #[cfg(feature = "dap")]
    pub(crate) debug: crate::dap::DebugState,
    /// The formatter tasks running in backend, which will be scheduled by async
    /// runtime.
    pub formatter: FormatTask,
    /// The user action tasks running in backend, which will be scheduled by
    /// async runtime.
    pub user_action: UserActionTask,

    // State to synchronize with the client.
    /// Whether the server has registered semantic tokens capabilities.
    pub sema_tokens_registered: bool,
    /// Whether the server has registered document formatter capabilities.
    pub formatter_registered: bool,
    /// Whether client is pinning a file.
    pub pinning_by_user: bool,
    /// Whether client is pinning caused by preview, which has lower priority
    /// than pinning.
    pub pinning_by_preview: bool,
    /// Whether client is pinning caused by preview, which has lower priority
    /// than pinning.
    pub pinning_by_browsing_preview: bool,
    /// The client focusing file.
    pub focusing: Option<ImmutPath>,
    /// The client focusing file.
    pub implicit_position: Option<LspPosition>,
    /// The client ever focused implicitly by activities.
    pub ever_focusing_by_activities: bool,
    /// The client ever sent manual focusing request.
    pub ever_manual_focusing: bool,

    // Configurations
    /// User configuration from the editor.
    pub config: Config,
    /// Source synchronized with client
    pub memory_changes: HashMap<Arc<Path>, Source>,
    /// The diagnostics sender to send diagnostics to `crate::actor::cluster`.
    pub editor_tx: mpsc::UnboundedSender<EditorRequest>,
}

/// Getters and the main loop.
impl ServerState {
    /// Create a new language server.
    pub fn new(
        client: TypedLspClient<ServerState>,
        config: Config,
        editor_tx: mpsc::UnboundedSender<EditorRequest>,
    ) -> Self {
        let formatter = FormatTask::new(config.formatter());

        #[cfg(feature = "preview")]
        let watchers = crate::project::ProjectPreviewState::default();
        let handle = Self::project(
            &config,
            editor_tx.clone(),
            client.clone(),
            #[cfg(feature = "preview")]
            watchers.clone(),
        );

        Self {
            client: client.clone(),
            route: ProjectRouteState::default(),
            project: handle,
            editor_tx,
            memory_changes: HashMap::new(),
            #[cfg(feature = "preview")]
            preview: tool::preview::PreviewState::new(
                &config,
                watchers,
                client.cast(|s| &mut s.preview),
            ),
            #[cfg(feature = "dap")]
            debug: crate::dap::DebugState::default(),
            ever_focusing_by_activities: false,
            ever_manual_focusing: false,
            sema_tokens_registered: false,
            formatter_registered: false,
            config,

            pinning_by_user: false,
            pinning_by_preview: false,
            pinning_by_browsing_preview: false,
            focusing: None,
            implicit_position: None,
            formatter,
            user_action: UserActionTask,
        }
    }

    /// Gets the const configuration.
    pub fn const_config(&self) -> &ConstConfig {
        &self.config.const_config
    }

    /// Gets the entry resolver.
    pub fn entry_resolver(&self) -> &EntryResolver {
        &self.config.entry_resolver
    }

    /// Whether the main file is pinning.
    pub fn is_pinning(&self) -> bool {
        self.pinning_by_user
            || (self.pinning_by_preview && {
                let primary_verse = &self.project.compiler.primary.verse;
                !primary_verse.entry_state().is_inactive()
            })
    }

    /// The entry point for the language server.
    pub fn main(client: TypedLspClient<Self>, config: Config, start: bool) -> Self {
        log::info!("ServerState: initialized with config {config:?}");

        // Bootstrap server
        let (editor_tx, editor_rx) = mpsc::unbounded_channel();

        let mut server = ServerState::new(client.clone(), config, editor_tx);

        if !server.config.warnings.is_empty() {
            server.show_config_warnings();
        }

        if start {
            let editor_actor = EditorActor::new(
                client.clone().to_untyped(),
                editor_rx,
                server.config.notify_status,
            );

            server
                .reload_projects()
                .log_error("could not restart primary");

            #[cfg(feature = "preview")]
            server.background_preview();

            // Run the cluster in the background after we referencing it
            client.handle.spawn(editor_actor.run());
        }

        server
    }

    /// Installs LSP handlers to the language server.
    pub fn install_lsp<T: Initializer<S = Self> + AddCommands + 'static>(
        provider: LspBuilder<T>,
    ) -> LspBuilder<T> {
        type State = ServerState;
        use lsp_types::notification::*;
        use lsp_types::request::*;

        #[cfg(feature = "preview")]
        let provider = provider
            // User commands
            .with_command("tinymist.startDefaultPreview", State::default_preview)
            .with_command("tinymist.scrollPreview", State::scroll_preview)
            // Internal commands
            .with_command("tinymist.doStartPreview", State::do_start_preview)
            .with_command("tinymist.doStartBrowsingPreview", State::browse_preview)
            .with_command("tinymist.doKillPreview", State::kill_preview);

        // todo: .on_sync_mut::<notifs::Cancel>(handlers::handle_cancel)?
        let mut provider = provider
            .with_request::<Shutdown>(State::shutdown)
            // customized event
            .with_event(
                &LspInterrupt::Compile(ProjectInsId::default()),
                State::compile_interrupt::<T>,
            )
            .with_event(
                &ServerEvent::UnpinPrimaryByPreview,
                State::server_event::<T>,
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
            // .with_command_("tinymist.exportSvgHtml", State::export_html)
            .with_command_("tinymist.exportPng", State::export_png)
            .with_command_("tinymist.exportText", State::export_text)
            .with_command_("tinymist.exportHtml", State::export_html)
            .with_command_("tinymist.exportMarkdown", State::export_markdown)
            .with_command_("tinymist.exportQuery", State::export_query)
            .with_command("tinymist.exportAnsiHighlight", State::export_ansi_hl)
            .with_command("tinymist.exportAst", State::export_ast)
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

    /// Installs DAP handlers to the language server.
    pub fn install_dap<T: Initializer<S = Self> + 'static>(
        provider: DapBuilder<T>,
    ) -> DapBuilder<T> {
        use dapts::request;

        // todo: .on_sync_mut::<notifs::Cancel>(handlers::handle_cancel)?
        provider
            .with_request::<request::ConfigurationDone>(Self::configuration_done)
            .with_request::<request::Disconnect>(Self::disconnect)
            .with_request::<request::Terminate>(Self::terminate_debug)
            .with_request::<request::TerminateThreads>(Self::terminate_debug_thread)
            .with_request::<request::Attach>(Self::attach_debug)
            .with_request::<request::Launch>(Self::launch_debug)
            .with_request::<request::Evaluate>(Self::evaluate_repl)
            .with_request::<request::Completions>(Self::complete_repl)
            .with_request::<request::Threads>(Self::debug_threads)
    }

    /// Handles the project interrupts.
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

    /// Handles the server events.
    fn server_event<T: Initializer<S = Self>>(
        mut state: ServiceState<T, T::S>,
        params: ServerEvent,
    ) -> anyhow::Result<()> {
        let _start = std::time::Instant::now();
        // log::info!("incoming interrupt: {params:?}");
        let Some(ready) = state.ready() else {
            log::info!("server event sent to not ready server");
            return Ok(());
        };

        match params {
            ServerEvent::UnpinPrimaryByPreview => {
                ready.set_pin_by_preview(false, false);
            }
        }

        Ok(())
    }

    #[cfg(feature = "preview")]
    pub(crate) fn infer_pos(&self) -> LspResult<typst_preview::ControlPlaneMessage> {
        use typst_preview::{ControlPlaneMessage, ResolveSourceLocRequest};

        let focus_file = self.focusing.as_ref();
        let focus_file = focus_file.ok_or_else(|| invalid_request("no focusing file"))?;

        let focus_location = self.implicit_position.as_ref();
        let focus_location =
            focus_location.ok_or_else(|| invalid_request("no focusing location"))?;

        Ok(ControlPlaneMessage::ResolveSourceLoc(
            ResolveSourceLocRequest {
                filepath: focus_file.as_ref().to_owned(),
                line: focus_location.line,
                character: focus_location.character,
            },
        ))
    }
}

/// An event sent to the language server.
pub enum ServerEvent {
    /// Updates the `pinning_by_preview` status to false.
    UnpinPrimaryByPreview,
}

impl ServerState {
    /// Shows the configuration warnings to the client.
    pub fn show_config_warnings(&mut self) {
        if !self.config.warnings.is_empty() {
            for warning in self.config.warnings.iter() {
                self.client.send_lsp_request::<ShowMessageRequest>(
                    ShowMessageRequestParams {
                        typ: MessageType::WARNING,
                        message: tinymist_l10n::t!(
                            "tinymist.config.badServerConfig",
                            "bad server configuration: {warning}",
                            warning = warning.as_ref().into()
                        )
                        .into(),
                        actions: None,
                    },
                    |_s, r| {
                        if let Some(err) = r.error {
                            log::error!("failed to send warning message: {err:?}");
                        }
                    },
                );
            }
        }
    }

    /// Gets the current server info.
    pub fn collect_server_info(&mut self) -> QueryFuture {
        let dg = self.project.primary_id().to_string();
        let api_stats = self.project.stats.report();
        let query_stats = self.project.analysis.report_query_stats();
        let alloc_stats = self.project.analysis.report_alloc_stats();

        let snap = self.snapshot()?;
        just_future(async move {
            let w = snap.world();

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

    /// Exports the current document.
    pub fn on_export(&mut self, req: OnExportRequest) -> QueryFuture {
        let OnExportRequest { path, task, open } = req;
        let entry = self.entry_resolver().resolve(Some(path.as_path().into()));
        let lock_dir = self.entry_resolver().resolve_lock(&entry);

        let update_dep = lock_dir.clone().map(|lock_dir| {
            |snap: LspComputeGraph| async move {
                let mut updater = update_lock(lock_dir);
                let world = snap.world();
                let doc_id = updater.compiled(world)?;

                updater.update_materials(doc_id.clone(), world.depended_fs_paths());
                updater.route(doc_id, PROJECT_ROUTE_USER_ACTION_PRIORITY);

                updater.commit();

                Some(())
            }
        });

        let snap = self.snapshot()?;
        just_future(async move {
            let snap = snap.task(TaskInputs {
                entry: Some(entry),
                ..TaskInputs::default()
            });

            let is_html = matches!(task, ProjectTask::ExportHtml { .. });
            let artifact = CompiledArtifact::from_graph(snap.clone(), is_html);
            let res = ExportTask::do_export(task, artifact, lock_dir).await?;
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
                log::trace!("open with system default apps: {path:?}");
                do_open(path).log_error("failed to open with system default apps");
            }

            log::trace!("CompileActor: on export end: {path:?} as {res:?}");
            Ok(tinymist_query::CompilerQueryResponse::OnExport(res))
        })
    }
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
