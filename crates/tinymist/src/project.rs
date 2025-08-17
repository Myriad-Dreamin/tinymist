//! The project.
//!
//! ```ascii
//! ┌────────────────────────────────┐         ┌────────────┐
//! │        main::main_loop         │◄───────►│notify_actor│
//! └─────┬────────────────────▲─────┘         └────────────┘
//!       │                    │
//! ┌─────▼────────────────────┴─────┐ handler ┌────────────┐
//! │   compiler::compile_handler    ├────────►│ rest actors│
//! └────────────────────────────────┘         └────────────┘
//! ```
//!
//! We use typst by creating a [`ProjectCompiler`] and
//! running compiler with callbacking [`CompileHandlerImpl`] incrementally. An
//! additional [`ProjectState`] is also created to control the
//! [`ProjectCompiler`].
//!
//! The [`CompileHandlerImpl`] will push information to other actors.

#![allow(missing_docs)]

#[cfg(feature = "web")]
use reflexo_typst::package::registry::ProxyContext;
use reflexo_typst::TypstDocument;
use serde::{Deserialize, Serialize};
pub use tinymist_project::*;

use std::sync::atomic::AtomicUsize;
use std::{num::NonZeroUsize, sync::Arc};

use parking_lot::Mutex;
use reflexo::hash::FxHashMap;
use sync_ls::{LspClient, TransportHost, TypedLspClient};
use tinymist_project::vfs::{FileChangeSet, MemoryEvent};
use tinymist_query::analysis::{Analysis, LspQuerySnapshot, PeriscopeProvider};
use tinymist_query::{
    CheckRequest, CompilerQueryRequest, DiagnosticsMap, LocalContext, SemanticRequest,
};
use tinymist_render::PeriscopeRenderer;
use tinymist_std::{error::prelude::*, ImmutPath};
use tokio::sync::mpsc;
use typst::{diag::FileResult, foundations::Bytes, layout::Position as TypstPosition};

use super::ServerState;
use crate::actor::editor::{EditorRequest, ProjVersion};
use crate::stats::{CompilerQueryStats, QueryStatGuard};
#[cfg(feature = "export")]
use crate::task::ExportUserConfig;
use crate::vfs::notify::NotifyMessage;
use crate::Config;
#[cfg(feature = "preview")]
use crate::ServerEvent;

type EditorSender = mpsc::UnboundedSender<EditorRequest>;

/// LSP project compiler.
pub type LspProjectCompiler = ProjectCompiler<LspCompilerFeat, ProjectInsStateExt>;

/// Project access and mutations.
impl ServerState {
    /// Changes the export configuration.
    #[cfg(feature = "export")]
    pub fn change_export_config(&mut self, config: ExportUserConfig) {
        self.project.export.change_config(config);
    }

    /// Snapshots the project for tasks
    pub fn snapshot(&mut self) -> Result<LspComputeGraph> {
        self.project.snapshot()
    }

    /// Snapshots the project for language queries
    pub fn query_snapshot(&mut self) -> Result<LspQuerySnapshot> {
        self.project.query_snapshot(None)
    }

    /// Snapshots the project for language queries
    pub fn query_snapshot_with_stat(
        &mut self,
        q: &CompilerQueryRequest,
    ) -> Result<QuerySnapWithStat> {
        let name: &'static str = q.into();
        let path = q.associated_path();
        let stat = self.project.stats.query_stat(path, name);
        let snap = self.project.query_snapshot(Some(q))?;
        Ok((snap, stat))
    }

    /// Reload the projects.
    pub fn reload_projects(&mut self) -> Result<()> {
        // todo: hot replacement
        #[cfg(feature = "preview")]
        self.preview.stop_all();
        let editor_tx = self.editor_tx.clone();

        #[cfg(feature = "web")]
        let new_project =
            if let TransportHost::Js { sender, .. } = self.client.clone().to_untyped().sender {
                Self::project(
                    &self.config,
                    editor_tx,
                    self.client.clone(),
                    #[cfg(feature = "preview")]
                    self.preview.watchers.clone(),
                    sender.resolve_fn,
                )
            } else {
                panic!("Expected Js TransportHost")
            };

        #[cfg(not(feature = "web"))]
        let new_project = Self::project(
            &self.config,
            editor_tx,
            self.client.clone(),
            self.dep_tx.clone(),
            #[cfg(feature = "preview")]
            self.preview.watchers.clone(),
        );

        let mut old_project = std::mem::replace(&mut self.project, new_project);

        // todo: the old dedicate projects should be transferred.

        let snapshot = FileChangeSet::new_inserts(
            self.memory_changes
                .iter()
                .map(|(path, content)| {
                    let content = Bytes::from_string(content.clone().text().to_owned());
                    (path.clone(), FileResult::Ok(content).into())
                })
                .collect(),
        );

        self.project
            .interrupt(Interrupt::Memory(MemoryEvent::Update(snapshot)));

        spawn_cpu(move || {
            old_project.stop();
        });

        Ok(())
    }

    /// Restarts a dedicate projects and returns corresponding instance id.
    pub fn restart_dedicate(
        &mut self,
        dedicate: &str,
        entry: Option<ImmutPath>,
    ) -> Result<ProjectInsId> {
        let entry = self.config.entry_resolver.resolve(entry);
        self.project.restart_dedicate(dedicate, entry)
    }

    /// Create a fresh [`ProjectState`].
    pub fn project(
        config: &Config,
        editor_tx: mpsc::UnboundedSender<EditorRequest>,
        client: TypedLspClient<ServerState>,
        dep_tx: mpsc::UnboundedSender<NotifyMessage>,
        #[cfg(feature = "preview")] preview: ProjectPreviewState,
        #[cfg(feature = "web")] resolve_fn: js_sys::Function,
    ) -> ProjectState {
        let const_config = &config.const_config;

        // Run Export actors before preparing cluster to avoid loss of events
        #[cfg(feature = "export")]
        let export = crate::task::ExportTask::new(
            client.handle.clone(),
            Some(editor_tx.clone()),
            config.export(),
        );

        // Create the compile handler for client consuming results.
        let periscope_args = config.periscope_args.clone();
        let handle = Arc::new(CompileHandlerImpl {
            #[cfg(feature = "preview")]
            preview,
            is_standalone: false,
            #[cfg(feature = "export")]
            export: export.clone(),
            editor_tx: editor_tx.clone(),
            client: Arc::new(client.clone().to_untyped()),
            analysis: Arc::new(Analysis {
                position_encoding: const_config.position_encoding,
                allow_overlapping_token: const_config.tokens_overlapping_token_support,
                allow_multiline_token: const_config.tokens_multiline_token_support,
                remove_html: !config.support_html_in_markdown,
                extended_code_action: config.extended_code_action,
                completion_feat: config.completion.clone(),
                color_theme: match config.color_theme.as_deref() {
                    Some("dark") => tinymist_query::ColorTheme::Dark,
                    _ => tinymist_query::ColorTheme::Light,
                },
                lint: config.lint.when().clone(),
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

            status_revision: Mutex::default(),
            notified_revision: Mutex::default(),
        });

        let export_target = config.export_target;
        let default_path = config.entry_resolver.resolve_default();
        let entry = config.entry_resolver.resolve(default_path);
        let inputs = config.inputs();
        let cert_path = config.certification_path();
        let package = config.package_opts();
        let features = config.typst_features().unwrap_or_default();
        let access_model = config.access_model(&client);

        log::info!("ServerState: creating ProjectState, entry: {entry:?}, inputs: {inputs:?}");

        let fonts = config.fonts();

        #[cfg(feature = "web")]
        let packages =
            LspUniverseBuilder::resolve_package(cert_path.clone(), Some(&package), resolve_fn);

        #[cfg(not(feature = "web"))]
        let packages = LspUniverseBuilder::resolve_package(cert_path.clone(), Some(&package));

        let creation_timestamp = config.creation_timestamp();
        let verse = LspUniverseBuilder::build(
            entry,
            export_target,
            features,
            inputs,
            packages,
            fonts,
            creation_timestamp,
            access_model,
        );

        // Creates the actor
        let compile_handle = handle.clone();
        let compiler = ProjectCompiler::new(
            verse,
            dep_tx,
            CompileServerOpts {
                handler: compile_handle,
                export_target: config.export_target,
                ignore_first_sync: true,
            },
        );

        ProjectState {
            compiler,
            #[cfg(feature = "preview")]
            preview: handle.preview.clone(),
            analysis: handle.analysis.clone(),
            stats: CompilerQueryStats::default(),
            #[cfg(feature = "export")]
            export: handle.export.clone(),
        }
    }
}

/// The extra state of a project instance.
#[derive(Default)]
pub struct ProjectInsStateExt {
    /// The revision notified to the compile handler.
    pub notified_revision: usize,
    /// The pending reasons that are not emitted yet during compilation.
    pub pending_reasons: CompileSignal,
    /// The emitted reasons that are emitted after the last compilation.
    pub emitted_reasons: CompileSignal,
    /// The compiling since the last compilation.
    pub compiling_since: Option<tinymist_std::time::Time>,
    /// The last compilation.
    pub last_compilation: Option<LspCompiledArtifact>,
}

impl ProjectInsStateExt {
    /// Remembers the last compilation. Emits the pending reasons during
    /// compilation if any.
    pub fn compiled(
        &mut self,
        revision: &NonZeroUsize,
        handler: &dyn CompileHandler<LspCompilerFeat, ProjectInsStateExt>,
        compilation: &LspCompiledArtifact,
    ) {
        self.compiling_since = None;

        let rev = compilation.world().revision().get();
        if self.notified_revision >= rev {
            return;
        }
        self.notified_revision = rev;

        self.last_compilation = Some(compilation.clone());

        self.emit_pending_reasons(revision, handler);
    }

    /// Emits the pending reasons if the latest compiled revision matches.
    pub fn emit_pending_reasons(
        &mut self,
        revision: &NonZeroUsize,
        handler: &dyn CompileHandler<LspCompilerFeat, ProjectInsStateExt>,
    ) -> bool {
        let Some(last_compilation) = self.last_compilation.as_ref() else {
            log::info!("skipping emit bcuz there's no last_compilation");
            return false;
        };

        let last_rev = last_compilation.world().revision();
        if last_rev != *revision {
            log::info!("skipping emit pending reasons for {revision:?} != {last_rev:?}");
            return false;
        }

        let pending_reasons = self.pending_reasons.exclude(self.emitted_reasons);
        if !pending_reasons.any() {
            log::info!("skipping emit bcuz there's no reason");
            return false;
        }
        self.emitted_reasons.merge(self.pending_reasons);
        let last_compilation = last_compilation.clone().with_signal(pending_reasons);

        handler.notify_compile(&last_compilation);
        self.pending_reasons = CompileSignal::default();

        true
    }
}

/// A project state.
pub struct ProjectState {
    /// The compiler instance.
    pub compiler: LspProjectCompiler,
    /// The analysis data.
    pub analysis: Arc<Analysis>,
    /// The query statistics.
    pub stats: CompilerQueryStats,
    /// The preview state.
    #[cfg(feature = "preview")]
    pub preview: ProjectPreviewState,
    /// The export task.
    #[cfg(feature = "export")]
    pub export: crate::task::ExportTask,
}

impl ProjectState {
    /// The primary instance id.
    pub fn primary_id(&self) -> &ProjectInsId {
        &self.compiler.primary.id
    }

    /// Snapshots the compiler thread for tasks.
    pub fn snapshot(&mut self) -> Result<LspComputeGraph> {
        Ok(self.compiler.snapshot())
    }

    /// Snapshots the compiler thread for language queries.
    pub fn query_snapshot(&mut self, q: Option<&CompilerQueryRequest>) -> Result<LspQuerySnapshot> {
        let snap = self.snapshot()?;
        Ok(self.analysis.clone().query_snapshot(snap, q))
    }

    /// Handles an interrupt.
    pub fn do_interrupt(compiler: &mut LspProjectCompiler, intr: Interrupt<LspCompilerFeat>) {
        if let Interrupt::Compiled(compiled) = &intr {
            let proj = compiler.projects().find(|p| &p.id == compiled.id());
            if let Some(proj) = proj {
                proj.ext
                    .compiled(&proj.verse.revision, proj.handler.as_ref(), compiled);
            } else {
                log::info!(
                    "Project({:?}): process compiled not found",
                    compiled.snap.id
                );
            }
        }

        compiler.process(intr);
    }

    /// Interrupts the compiler.
    pub fn interrupt(&mut self, intr: Interrupt<LspCompilerFeat>) {
        Self::do_interrupt(&mut self.compiler, intr);
    }

    /// Stops the project.
    pub(crate) fn stop(&mut self) {
        // todo: stop all compilations
    }

    /// Restarts a dedicate project.
    pub(crate) fn restart_dedicate(
        &mut self,
        group: &str,
        entry: EntryState,
    ) -> Result<ProjectInsId> {
        self.compiler.restart_dedicate(group, entry)
    }
}

/// The implementation of the periscope provider.
struct TypstPeriscopeProvider(PeriscopeRenderer);

impl PeriscopeProvider for TypstPeriscopeProvider {
    /// Resolves the periscope image at the given position.
    fn periscope_at(
        &self,
        ctx: &mut LocalContext,
        doc: &TypstDocument,
        pos: TypstPosition,
    ) -> Option<String> {
        self.0.render_marked(ctx, doc, pos)
    }
}

/// The preview state of a project.
#[derive(Default, Clone)]
pub struct ProjectPreviewState {
    /// The inner state.
    #[cfg(feature = "preview")]
    pub(crate) inner: Arc<Mutex<FxHashMap<ProjectInsId, Arc<tinymist_preview::CompileWatcher>>>>,
}

#[cfg(feature = "preview")]
impl ProjectPreviewState {
    /// Registers a compile watcher.
    #[must_use]
    pub fn register(
        &self,
        id: &ProjectInsId,
        handle: &Arc<tinymist_preview::CompileWatcher>,
    ) -> bool {
        let mut p = self.inner.lock();

        // Don't replace the existing watcher if it exists
        if p.contains_key(id) {
            return false;
        }

        p.insert(id.clone(), handle.clone());
        true
    }

    /// Unregisters a compile watcher.
    #[must_use]
    pub fn unregister(&self, task_id: &ProjectInsId) -> bool {
        self.inner.lock().remove(task_id).is_some()
    }

    /// Gets a compile watcher.
    #[must_use]
    pub fn get(&self, task_id: &ProjectInsId) -> Option<Arc<tinymist_preview::CompileWatcher>> {
        self.inner.lock().get(task_id).cloned()
    }
}

/// The implementation of the compile handler.
pub struct CompileHandlerImpl {
    /// The analysis data.
    pub(crate) analysis: Arc<Analysis>,

    #[cfg(feature = "preview")]
    pub(crate) preview: ProjectPreviewState,
    /// Whether the compile server is running in standalone CLI (not as a
    /// language server).
    pub is_standalone: bool,
    /// The export task.
    #[cfg(feature = "export")]
    pub(crate) export: crate::task::ExportTask,
    /// The editor sender, used to send editor requests to the editor.
    pub(crate) editor_tx: EditorSender,
    /// The client used to send events back to the server itself or the clients.
    pub(crate) client: Arc<dyn ProjectClient>,
    /// The status revision map, used to track the status of the projects.
    pub(crate) status_revision: Mutex<FxHashMap<ProjectInsId, usize>>,
    /// The notified revision map, used to track the notified revisions of the
    /// projects.
    pub(crate) notified_revision: Mutex<FxHashMap<ProjectInsId, (usize, CompileSignal)>>,
}

/// The client of the project.
pub trait ProjectClient: Send + Sync + 'static {
    /// Sends an interrupt event back to the server.
    fn interrupt(&self, event: LspInterrupt);
    /// Sends a server event back to the server.
    #[cfg(feature = "preview")]
    fn server_event(&self, event: ServerEvent);
    /// Sends a dev event to the client, used for neovim's E2E testing.
    #[cfg(feature = "export")]
    fn dev_event(&self, event: DevEvent);
}

impl ProjectClient for LspClient {
    fn interrupt(&self, event: LspInterrupt) {
        self.send_event(event);
    }

    #[cfg(feature = "preview")]
    fn server_event(&self, event: ServerEvent) {
        self.send_event(event);
    }

    #[cfg(feature = "export")]
    fn dev_event(&self, event: DevEvent) {
        self.send_notification::<DevEvent>(&event);
    }
}

impl ProjectClient for mpsc::UnboundedSender<LspInterrupt> {
    fn interrupt(&self, event: LspInterrupt) {
        self.send(event).log_error("failed to send interrupt");
    }

    #[cfg(feature = "preview")]
    fn server_event(&self, _event: ServerEvent) {
        log::warn!("ProjectClient: server_event is not implemented for mpsc::UnboundedSender<LspInterrupt>");
    }

    #[cfg(feature = "export")]
    fn dev_event(&self, _event: DevEvent) {
        log::warn!(
            "ProjectClient: dev_event is not implemented for mpsc::UnboundedSender<LspInterrupt>"
        );
    }
}

impl CompileHandlerImpl {
    /// Pushes diagnostics to the editor.
    fn push_diagnostics(&self, dv: ProjVersion, diagnostics: Option<DiagnosticsMap>) {
        self.editor_tx
            .send(EditorRequest::Diag(dv, diagnostics))
            .log_error("failed to send diagnostics");
    }

    /// Notifies the diagnostics.
    fn notify_diagnostics(&self, art: &LspCompiledArtifact) {
        let dv = ProjVersion {
            id: art.id().clone(),
            revision: art.world().revision().get(),
        };
        // todo: better way to remove diagnostics
        let valid = !art.world().entry_state().is_inactive();
        if !valid {
            self.push_diagnostics(dv, None);
            return;
        }

        let should_lint = art
            .snap
            .signal
            .should_run_task_dyn(&self.analysis.lint, art.doc.as_ref())
            .unwrap_or_default();
        log::debug!(
            "Project: should_lint: {should_lint:?}, signal: {:?}",
            art.snap.signal
        );

        if !should_lint {
            let enc = self.analysis.position_encoding;
            let diagnostics =
                tinymist_query::convert_diagnostics(art.world(), art.diagnostics(), enc);

            log::trace!("notify diagnostics({dv:?}): {diagnostics:#?}");

            self.editor_tx
                .send(EditorRequest::Diag(dv, Some(diagnostics)))
                .log_error("failed to send diagnostics");
        } else {
            let snap = art.clone();
            let editor_tx = self.editor_tx.clone();
            let analysis = self.analysis.clone();
            spawn_cpu(move || {
                let world = snap.world().clone();
                let mut ctx = analysis.enter(world);

                // todo: check all errors in this file
                let Some(diagnostics) = CheckRequest { snap }.request(&mut ctx) else {
                    return;
                };

                log::trace!("notify diagnostics({dv:?}): {diagnostics:#?}");

                editor_tx
                    .send(EditorRequest::Diag(dv, Some(diagnostics)))
                    .log_error("failed to send diagnostics");
            });
        }
    }
}

impl CompileHandler<LspCompilerFeat, ProjectInsStateExt> for CompileHandlerImpl {
    fn on_any_compile_reason(&self, c: &mut LspProjectCompiler) {
        let instances_mut = std::iter::once(&mut c.primary).chain(c.dedicates.iter_mut());
        for s in instances_mut {
            let reason = s.reason;
            if !reason.any() {
                continue;
            }

            let id = &s.id;

            if let Some(compiling_since) = &s.ext.compiling_since {
                static CHECK_COMPILE_TIMEOUT: AtomicUsize = AtomicUsize::new(0);
                let check_stalled = (CHECK_COMPILE_TIMEOUT
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    & 0xFF)
                    == 0;

                if check_stalled {
                    let since = (tinymist_std::time::now().duration_since(*compiling_since))
                        .unwrap_or_default();

                    if since.as_secs() > 60 {
                        log::warn!(
                        "Project({id:?}): compiling for more than 60 seconds, since: {since:?}, \
                     pending reasons: {:?}",
                        s.ext.pending_reasons
                    );
                    }
                }

                continue;
            }

            const VFS_SUB: CompileSignal = CompileSignal {
                by_mem_events: true,
                by_fs_events: true,
                by_entry_update: false,
            };

            let is_vfs_sub = reason.any() && !reason.exclude(VFS_SUB).any();

            if is_vfs_sub
                && 'vfs_is_clean: {
                    let Some(compilation) = &s.ext.last_compilation else {
                        break 'vfs_is_clean false;
                    };
                    if compilation.world().entry_state() != s.verse.entry_state() {
                        log::info!("Project: updated regardless of vfs for {id:?} due to entry state change, world: {:?} v.s. verse: {:?}",
                            compilation.world().entry_state(),
                            s.verse.entry_state(),
                        );
                        break 'vfs_is_clean false;
                    }

                    let last_rev = compilation.world().vfs().revision();
                    let deps = compilation.depended_files().clone();
                    s.verse.vfs().is_clean_compile(last_rev.get(), &deps)
                }
            {
                s.ext.pending_reasons.merge(reason);
                s.reason = CompileSignal::default();

                let pending_reasons = s.ext.pending_reasons.exclude(s.ext.emitted_reasons);
                let emitted = s
                    .ext
                    .emit_pending_reasons(&s.verse.revision, s.handler.as_ref());

                if !emitted {
                    log::info!("Project: skip compilation for {id:?} due to harmless vfs changes");
                } else {
                    log::info!(
                        "Project: emit compilation again for {id:?}, reason: {pending_reasons:?}"
                    );
                }

                continue;
            }

            s.ext.pending_reasons = CompileSignal::default();
            s.ext.emitted_reasons = reason;
            let Some(compile_fn) = s.may_compile(&c.handler) else {
                continue;
            };

            s.ext.compiling_since = Some(tinymist_std::time::now());
            spawn_cpu(move || {
                compile_fn();
            });
        }
    }

    fn status(&self, revision: usize, rep: CompileReport) {
        {
            let mut n_revs = self.status_revision.lock();
            let n_rev = n_revs.entry(rep.id.clone()).or_default();
            if *n_rev > revision {
                log::info!("Project: outdated status for revision {revision} <= {n_rev}");
                return;
            }
            *n_rev = revision;
        }

        if matches!(rep.status, tinymist_project::CompileStatusEnum::Suspend) {
            let dv = ProjVersion {
                id: rep.id.clone(),
                revision,
            };
            self.push_diagnostics(dv, None);
        }

        #[cfg(feature = "preview")]
        if let Some(inner) = self.preview.get(&rep.id) {
            use tinymist_preview::CompileStatus;
            use tinymist_project::CompileStatusEnum::*;

            inner.status(match &rep.status {
                Compiling => CompileStatus::Compiling,
                Suspend | CompileSuccess { .. } => CompileStatus::CompileSuccess,
                ExportError { .. } | CompileError { .. } => CompileStatus::CompileError,
            });
        }

        self.editor_tx.send(EditorRequest::Status(rep)).unwrap();
    }

    fn notify_removed(&self, id: &ProjectInsId) {
        let n_revs = &mut self.notified_revision.lock();
        let last_rev = n_revs.remove(id).unwrap_or_default();

        let dv = ProjVersion {
            id: id.clone(),
            revision: last_rev.0,
        };

        // todo: race condition with notify_compile?
        // remove diagnostics
        self.push_diagnostics(dv, None);
    }

    fn notify_compile(&self, art: &LspCompiledArtifact) {
        // NOTE: we have to inform the main thread about the compilation. If such
        // interrupt is not sent, the main thread will be stalled forever.
        self.client.interrupt(LspInterrupt::Compiled(art.clone()));

        {
            let mut n_revs = self.notified_revision.lock();
            let (n_rev, n_signal) = n_revs.entry(art.id().clone()).or_default();

            let rev = art.world().revision().get();

            // If the revision is outdated, update it and notify the client.
            if *n_rev < rev {
                *n_rev = rev;
                // The signal must be reset to the current one.
                *n_signal = art.snap.signal;

                // Otherwise,
                // 1. if the revision is not the same, ignores the signal.
                // 2. if a fresh signal is found, merges and emits it.
            } else if *n_rev == rev && n_signal.exclude(art.snap.signal).any() {
                n_signal.merge(art.snap.signal);

                // Otherwise we have already notified the client for this
                // revision.
            } else {
                log::info!(
                    "Project: already notified for revision {} <= {n_rev}, signal: {n_signal:?} contains {:?}",
                    art.world().revision(),
                    art.snap.signal
                );
                return;
            }
        }

        // Prints the diagnostics when we are running the compilation in standalone
        // CLI.
        #[cfg(feature = "system")]
        if self.is_standalone {
            crate::project::system::print_diagnostics(
                art.world(),
                art.diagnostics(),
                reflexo_typst::DiagnosticFormat::Human,
            )
            .log_error("failed to print diagnostics");
        }

        #[cfg(feature = "export")]
        self.export.signal(art, &self.client);

        #[cfg(feature = "preview")]
        if let Some(inner) = self.preview.get(art.id()) {
            let art = art.clone();
            inner.notify_compile(Arc::new(crate::tool::preview::PreviewCompileView { art }));
        } else {
            log::debug!("Project: no preview for {:?}", art.id());
        }

        self.notify_diagnostics(art);
    }
}

/// A query snapshot with statistics.
pub type QuerySnapWithStat = (LspQuerySnapshot, QueryStatGuard);

/// A notification event that an export was checked.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevExportEvent {
    /// The project id.
    pub id: String,
    /// The configured timing to execute the export.
    pub when: TaskWhen,
    /// Whether the export is needed.
    pub need_export: bool,
    /// The signal to check if the export is needed.
    pub signal: CompileSignal,
    /// The path to write the exported artifact.
    ///
    /// If `None`, the artifact will be written to the default path according
    /// to the input path.
    pub path: Option<String>,
}

/// A notification event that a dev event was triggered, used for neovim's E2E
/// testing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum DevEvent {
    /// A notification event that an export was triggered.
    Export(DevExportEvent),
}

impl lsp_types::notification::Notification for DevEvent {
    const METHOD: &'static str = "tinymist/devEvent";
    type Params = Self;
}
