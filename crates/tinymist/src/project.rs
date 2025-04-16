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

use reflexo_typst::{diag::print_diagnostics, TypstDocument};
pub use tinymist_project::*;

use std::{num::NonZeroUsize, sync::Arc};

use parking_lot::Mutex;
use reflexo::hash::FxHashMap;
use sync_ls::{LspClient, TypedLspClient};
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
use crate::task::ExportUserConfig;
use crate::{Config, ServerEvent};

type EditorSender = mpsc::UnboundedSender<EditorRequest>;

/// LSP project compiler.
pub type LspProjectCompiler = ProjectCompiler<LspCompilerFeat, ProjectInsStateExt>;

/// Project access and mutations.
impl ServerState {
    /// Changes the export configuration.
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

        let new_project = Self::project(
            &self.config,
            editor_tx,
            self.client.clone(),
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

        rayon::spawn(move || {
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
        #[cfg(feature = "preview")] preview: ProjectPreviewState,
    ) -> ProjectState {
        let const_config = &config.const_config;

        // Run Export actors before preparing cluster to avoid loss of events
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
            export: export.clone(),
            editor_tx: editor_tx.clone(),
            client: Box::new(client.clone().to_untyped()),
            analysis: Arc::new(Analysis {
                position_encoding: const_config.position_encoding,
                allow_overlapping_token: const_config.tokens_overlapping_token_support,
                allow_multiline_token: const_config.tokens_multiline_token_support,
                remove_html: !config.support_html_in_markdown,
                completion_feat: config.completion.clone(),
                color_theme: match config.color_theme.as_deref() {
                    Some("dark") => tinymist_query::ColorTheme::Dark,
                    _ => tinymist_query::ColorTheme::Light,
                },
                lint: config.lint.when(),
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

        log::info!("ServerState: creating ProjectState, entry: {entry:?}, inputs: {inputs:?}");

        let fonts = config.fonts();
        let packages = LspUniverseBuilder::resolve_package(cert_path.clone(), Some(&package));
        let verse =
            LspUniverseBuilder::build(entry, export_target, features, inputs, packages, fonts);

        // todo: unify filesystem watcher
        let (dep_tx, dep_rx) = mpsc::unbounded_channel();
        let fs_client = client.clone().to_untyped();
        let async_handle = client.handle.clone();
        async_handle.spawn(watch_deps(dep_rx, move |event| {
            fs_client.send_event(LspInterrupt::Fs(event));
        }));

        // Create the actor
        let compile_handle = handle.clone();
        let compiler = ProjectCompiler::new(
            verse,
            dep_tx,
            CompileServerOpts {
                handler: compile_handle,
                export_target: config.export_target,
                enable_watch: true,
            },
        );

        ProjectState {
            compiler,
            preview: handle.preview.clone(),
            analysis: handle.analysis.clone(),
            stats: CompilerQueryStats::default(),
            export: handle.export.clone(),
        }
    }
}

#[derive(Default)]
pub struct ProjectInsStateExt {
    pub notified_revision: usize,
    pub pending_reasons: CompileReasons,
    pub emitted_reasons: CompileReasons,
    pub is_compiling: bool,
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
        let rev = compilation.world().revision().get();
        if self.notified_revision >= rev {
            return;
        }
        self.notified_revision = rev;

        self.is_compiling = false;
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
            return false;
        };

        let last_rev = last_compilation.world().revision();
        if last_rev != *revision {
            return false;
        }

        let pending_reasons = self.pending_reasons.exclude(self.emitted_reasons);
        if !pending_reasons.any() {
            return false;
        }
        self.emitted_reasons.see(self.pending_reasons);
        let last_compilation = last_compilation.clone().with_signal(pending_reasons.into());

        handler.notify_compile(&last_compilation);
        self.pending_reasons = CompileReasons::default();

        true
    }
}

pub struct ProjectState {
    pub compiler: LspProjectCompiler,
    pub preview: ProjectPreviewState,
    pub analysis: Arc<Analysis>,
    pub stats: CompilerQueryStats,
    pub export: crate::task::ExportTask,
}

impl ProjectState {
    /// The primary instance id
    pub fn primary_id(&self) -> &ProjectInsId {
        &self.compiler.primary.id
    }

    /// Snapshot the compiler thread for tasks
    pub fn snapshot(&mut self) -> Result<LspComputeGraph> {
        Ok(self.compiler.snapshot())
    }

    /// Snapshot the compiler thread for language queries
    pub fn query_snapshot(&mut self, q: Option<&CompilerQueryRequest>) -> Result<LspQuerySnapshot> {
        let snap = self.snapshot()?;
        Ok(self.analysis.clone().query_snapshot(snap, q))
    }

    pub fn do_interrupt(compiler: &mut LspProjectCompiler, intr: Interrupt<LspCompilerFeat>) {
        if let Interrupt::Compiled(compiled) = &intr {
            let proj = compiler.projects().find(|p| &p.id == compiled.id());
            if let Some(proj) = proj {
                proj.ext
                    .compiled(&proj.verse.revision, proj.handler.as_ref(), compiled);
            }
        }

        compiler.process(intr);
    }

    pub fn interrupt(&mut self, intr: Interrupt<LspCompilerFeat>) {
        Self::do_interrupt(&mut self.compiler, intr);
    }

    pub(crate) fn stop(&mut self) {
        // todo: stop all compilations
    }

    pub(crate) fn restart_dedicate(
        &mut self,
        group: &str,
        entry: EntryState,
    ) -> Result<ProjectInsId> {
        self.compiler.restart_dedicate(group, entry)
    }
}

struct TypstPeriscopeProvider(PeriscopeRenderer);

impl PeriscopeProvider for TypstPeriscopeProvider {
    /// Resolve periscope image at the given position.
    fn periscope_at(
        &self,
        ctx: &mut LocalContext,
        doc: &TypstDocument,
        pos: TypstPosition,
    ) -> Option<String> {
        self.0.render_marked(ctx, doc, pos)
    }
}

#[derive(Default, Clone)]
pub struct ProjectPreviewState {
    #[cfg(feature = "preview")]
    pub(crate) inner: Arc<Mutex<FxHashMap<ProjectInsId, Arc<typst_preview::CompileWatcher>>>>,
}

#[cfg(feature = "preview")]
impl ProjectPreviewState {
    #[must_use]
    pub fn register(&self, id: &ProjectInsId, handle: &Arc<typst_preview::CompileWatcher>) -> bool {
        let mut p = self.inner.lock();

        // Don't replace the existing watcher if it exists
        if p.contains_key(id) {
            return false;
        }

        p.insert(id.clone(), handle.clone());
        true
    }

    #[must_use]
    pub fn unregister(&self, task_id: &ProjectInsId) -> bool {
        self.inner.lock().remove(task_id).is_some()
    }

    #[must_use]
    pub fn get(&self, task_id: &ProjectInsId) -> Option<Arc<typst_preview::CompileWatcher>> {
        self.inner.lock().get(task_id).cloned()
    }
}

pub struct CompileHandlerImpl {
    pub(crate) analysis: Arc<Analysis>,

    #[cfg(feature = "preview")]
    pub(crate) preview: ProjectPreviewState,
    /// Whether the compile server is running in standalone CLI (not as a
    /// language server).
    pub is_standalone: bool,

    pub(crate) export: crate::task::ExportTask,
    pub(crate) editor_tx: EditorSender,
    pub(crate) client: Box<dyn ProjectClient>,

    pub(crate) status_revision: Mutex<FxHashMap<ProjectInsId, usize>>,
    pub(crate) notified_revision: Mutex<FxHashMap<ProjectInsId, usize>>,
}

pub trait ProjectClient: Send + Sync + 'static {
    fn interrupt(&self, event: LspInterrupt);
    fn server_event(&self, event: ServerEvent);
}

impl ProjectClient for LspClient {
    fn interrupt(&self, event: LspInterrupt) {
        self.send_event(event);
    }

    fn server_event(&self, event: ServerEvent) {
        self.send_event(event);
    }
}

impl ProjectClient for mpsc::UnboundedSender<LspInterrupt> {
    fn interrupt(&self, event: LspInterrupt) {
        self.send(event).log_error("failed to send interrupt");
    }

    fn server_event(&self, _event: ServerEvent) {
        log::warn!("ProjectClient: server_event is not implemented for mpsc::UnboundedSender<LspInterrupt>");
    }
}

impl CompileHandlerImpl {
    fn push_diagnostics(&self, dv: ProjVersion, diagnostics: Option<DiagnosticsMap>) {
        self.editor_tx
            .send(EditorRequest::Diag(dv, diagnostics))
            .log_error("failed to send diagnostics");
    }

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
            .should_run_task_dyn(self.analysis.lint, art.doc.as_ref())
            .unwrap_or_default();
        log::info!(
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
            rayon::spawn(move || {
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
            if s.ext.is_compiling {
                continue;
            }

            let reason = s.reason;

            const VFS_SUB: CompileReasons = CompileReasons {
                by_memory_events: true,
                by_fs_events: true,
                by_entry_update: false,
            };

            let is_vfs_sub = reason.any() && !reason.exclude(VFS_SUB).any();
            let id = &s.id;

            if is_vfs_sub
                && 'vfs_is_clean: {
                    let Some(compilation) = &s.ext.last_compilation else {
                        break 'vfs_is_clean false;
                    };

                    let last_rev = compilation.world().vfs().revision();
                    let deps = compilation.depended_files().clone();
                    s.verse.vfs().is_clean_compile(last_rev.get(), &deps)
                }
            {
                s.ext.pending_reasons.see(reason);
                s.reason = CompileReasons::default();

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

            s.ext.pending_reasons = CompileReasons::default();
            s.ext.emitted_reasons = reason;
            let Some(compile_fn) = s.may_compile(&c.handler) else {
                continue;
            };
            s.ext.is_compiling = true;
            rayon::spawn(move || {
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
            use tinymist_project::CompileStatusEnum::*;
            use typst_preview::CompileStatus;

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
            revision: last_rev,
        };

        // todo: race condition with notify_compile?
        // remove diagnostics
        self.push_diagnostics(dv, None);
    }

    fn notify_compile(&self, art: &LspCompiledArtifact) {
        {
            let mut n_revs = self.notified_revision.lock();
            let n_rev = n_revs.entry(art.id().clone()).or_default();
            if *n_rev >= art.world().revision().get() {
                log::info!(
                    "Project: already notified for revision {} <= {n_rev}",
                    art.world().revision(),
                );
                return;
            }
            *n_rev = art.world().revision().get();
        }

        // Prints the diagnostics when we are running the compilation in standalone
        // CLI.
        if self.is_standalone {
            print_diagnostics(
                art.world(),
                art.diagnostics(),
                reflexo_typst::DiagnosticFormat::Human,
            )
            .log_error("failed to print diagnostics");
        }

        self.client.interrupt(LspInterrupt::Compiled(art.clone()));
        self.export.signal(art);

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

pub type QuerySnapWithStat = (LspQuerySnapshot, QueryStatGuard);
