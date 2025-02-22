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

use reflexo_typst::diag::print_diagnostics;
pub use tinymist_project::*;

use std::{num::NonZeroUsize, sync::Arc};

use parking_lot::Mutex;
use reflexo::{hash::FxHashMap, path::unix_slash};
use sync_lsp::{LspClient, TypedLspClient};
use tinymist_project::vfs::{FileChangeSet, MemoryEvent};
use tinymist_query::{
    analysis::{Analysis, AnalysisRevLock, LocalContextGuard, PeriscopeProvider},
    CompilerQueryRequest, CompilerQueryResponse, DiagnosticsMap, LocalContext, SemanticRequest,
    StatefulRequest, VersionedDocument,
};
use tinymist_render::PeriscopeRenderer;
use tinymist_std::{error::prelude::*, ImmutPath};
use tokio::sync::mpsc;
use typst::{diag::FileResult, foundations::Bytes, layout::Position as TypstPosition};

use super::ServerState;
use crate::stats::{CompilerQueryStats, QueryStatGuard};
use crate::{
    actor::editor::{CompileStatus, CompileStatusEnum, EditorRequest, ProjVersion},
    ServerEvent,
};
use crate::{task::ExportUserConfig, Config};

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
    pub fn snapshot(&mut self) -> Result<LspCompileSnapshot> {
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
        let entry = self.config.compile.entry_resolver.resolve(entry);
        let enable_html = matches!(self.config.export_target, ExportTarget::Html);
        self.project.restart_dedicate(dedicate, entry, enable_html)
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
        let periscope_args = config.compile.periscope_args.clone();
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
                color_theme: match config.compile.color_theme.as_deref() {
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

            status_revision: Mutex::default(),
            notified_revision: Mutex::default(),
        });

        let export_target = config.export_target;
        let default_path = config.compile.entry_resolver.resolve_default();
        let entry = config.compile.entry_resolver.resolve(default_path);
        let inputs = config.compile.determine_inputs();
        let cert_path = config.compile.determine_certification_path();
        let package = config.compile.determine_package_opts();

        log::info!("ServerState: creating ProjectState, entry: {entry:?}, inputs: {inputs:?}");

        // todo: never fail?
        let embedded_fonts = Arc::new(LspUniverseBuilder::only_embedded_fonts().unwrap());
        let package_registry =
            LspUniverseBuilder::resolve_package(cert_path.clone(), Some(&package));
        let verse = LspUniverseBuilder::build(
            entry,
            export_target,
            inputs,
            embedded_fonts,
            package_registry,
        );

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
                enable_watch: true,
            },
        );

        // Delayed Loads fonts
        let font_client = client.clone();
        let font_resolver = config.compile.determine_fonts();
        client.handle.spawn_blocking(move || {
            // Refresh fonts
            font_client.send_event(LspInterrupt::Font(font_resolver.wait().clone()));
        });

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
        let rev = compilation.world.revision().get();
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

        let last_rev = last_compilation.world.revision();
        if last_rev != *revision {
            return false;
        }

        let pending_reasons = self.pending_reasons.exclude(self.emitted_reasons);
        if !pending_reasons.any() {
            return false;
        }
        self.emitted_reasons.see(self.pending_reasons);
        let mut last_compilation = last_compilation.clone();
        last_compilation.snap.signal = pending_reasons.into();

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
    pub fn snapshot(&mut self) -> Result<LspCompileSnapshot> {
        Ok(self.compiler.snapshot())
    }

    /// Snapshot the compiler thread for language queries
    pub fn query_snapshot(&mut self, q: Option<&CompilerQueryRequest>) -> Result<LspQuerySnapshot> {
        let snap = self.snapshot()?;
        let analysis = self.analysis.clone();
        let rev_lock = analysis.lock_revision(q);

        Ok(LspQuerySnapshot {
            snap,
            analysis,
            rev_lock,
        })
    }

    pub fn do_interrupt(compiler: &mut LspProjectCompiler, intr: Interrupt<LspCompilerFeat>) {
        if let Interrupt::Compiled(compiled) = &intr {
            let proj = compiler.projects().find(|p| p.id == compiled.id);
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
        enable_html: bool,
    ) -> Result<ProjectInsId> {
        self.compiler.restart_dedicate(group, entry, enable_html)
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

    fn notify_diagnostics(&self, snap: &LspCompiledArtifact) {
        let world = &snap.world;
        let dv = ProjVersion {
            id: snap.id.clone(),
            revision: world.revision().get(),
        };

        // todo: better way to remove diagnostics
        // todo: check all errors in this file
        let valid = !world.entry_state().is_inactive();
        let diagnostics = valid.then(|| {
            let errors = snap.doc.as_ref().err().into_iter().flatten();
            let warnings = snap.warnings.as_ref();
            let diagnostics = tinymist_query::convert_diagnostics(
                world,
                errors.chain(warnings),
                self.analysis.position_encoding,
            );

            log::trace!("notify diagnostics({dv:?}): {diagnostics:#?}");
            diagnostics
        });

        self.push_diagnostics(dv, diagnostics);
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

                    let last_rev = compilation.world.vfs().revision();
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

    fn status(&self, revision: usize, id: &ProjectInsId, rep: CompileReport) {
        {
            let mut n_revs = self.status_revision.lock();
            let n_rev = n_revs.entry(id.clone()).or_default();
            if *n_rev > revision {
                log::info!("Project: outdated status for revision {revision} <= {n_rev}");
                return;
            }
            *n_rev = revision;
        }

        // todo: seems to duplicate with CompileStatus
        let status = match rep {
            CompileReport::Suspend => {
                let dv = ProjVersion {
                    id: id.clone(),
                    revision,
                };
                self.push_diagnostics(dv, None);
                CompileStatusEnum::CompileSuccess
            }
            CompileReport::Stage(_, _, _) => CompileStatusEnum::Compiling,
            CompileReport::CompileSuccess(_, _, _) => CompileStatusEnum::CompileSuccess,
            CompileReport::CompileError(_, _, _) | CompileReport::ExportError(_, _, _) => {
                CompileStatusEnum::CompileError
            }
        };

        let this = &self;
        this.editor_tx
            .send(EditorRequest::Status(CompileStatus {
                id: id.clone(),
                path: rep
                    .compiling_id()
                    .map(|s| unix_slash(s.vpath().as_rooted_path()))
                    .unwrap_or_default(),
                status,
            }))
            .unwrap();

        #[cfg(feature = "preview")]
        if let Some(inner) = this.preview.get(id) {
            use typst_preview::CompileStatus;

            let status = match rep {
                CompileReport::Suspend => CompileStatus::CompileSuccess,
                CompileReport::Stage(_, _, _) => CompileStatus::Compiling,
                CompileReport::CompileSuccess(_, _, _) => CompileStatus::CompileSuccess,
                CompileReport::CompileError(_, _, _) | CompileReport::ExportError(_, _, _) => {
                    CompileStatus::CompileError
                }
            };

            inner.status(status);
        }
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

    fn notify_compile(&self, snap: &LspCompiledArtifact) {
        {
            let mut n_revs = self.notified_revision.lock();
            let n_rev = n_revs.entry(snap.id.clone()).or_default();
            if *n_rev >= snap.world.revision().get() {
                log::info!(
                    "Project: already notified for revision {} <= {n_rev}",
                    snap.world.revision(),
                );
                return;
            }
            *n_rev = snap.world.revision().get();
        }

        // Prints the diagnostics when we are running the compilation in standalone
        // CLI.
        if self.is_standalone {
            print_diagnostics(
                &snap.world,
                snap.doc
                    .as_ref()
                    .err()
                    .cloned()
                    .iter()
                    .flatten()
                    .chain(snap.warnings.iter()),
                reflexo_typst::DiagnosticFormat::Human,
            )
            .log_error("failed to print diagnostics");
        }

        self.notify_diagnostics(snap);

        self.client.interrupt(LspInterrupt::Compiled(snap.clone()));
        self.export.signal(snap);

        #[cfg(feature = "preview")]
        if let Some(inner) = self.preview.get(&snap.id) {
            let snap = snap.clone();
            inner.notify_compile(Arc::new(crate::tool::preview::PreviewCompileView { snap }));
        } else {
            log::info!("Project: no preview for {:?}", snap.id);
        }
    }
}

pub type QuerySnapWithStat = (LspQuerySnapshot, QueryStatGuard);

pub struct LspQuerySnapshot {
    pub snap: LspCompileSnapshot,
    analysis: Arc<Analysis>,
    rev_lock: AnalysisRevLock,
}

impl std::ops::Deref for LspQuerySnapshot {
    type Target = LspCompileSnapshot;

    fn deref(&self) -> &Self::Target {
        &self.snap
    }
}

impl LspQuerySnapshot {
    pub fn task(mut self, inputs: TaskInputs) -> Self {
        self.snap = self.snap.task(inputs);
        self
    }

    pub fn run_stateful<T: StatefulRequest>(
        self,
        query: T,
        wrapper: fn(Option<T::Response>) -> CompilerQueryResponse,
    ) -> Result<CompilerQueryResponse> {
        let doc = self.snap.success_doc.as_ref().map(|doc| VersionedDocument {
            version: self.world.revision().get(),
            document: doc.clone(),
        });
        self.run_analysis(|ctx| query.request(ctx, doc))
            .map(wrapper)
    }

    pub fn run_semantic<T: SemanticRequest>(
        self,
        query: T,
        wrapper: fn(Option<T::Response>) -> CompilerQueryResponse,
    ) -> Result<CompilerQueryResponse> {
        self.run_analysis(|ctx| query.request(ctx)).map(wrapper)
    }

    pub fn run_analysis<T>(self, f: impl FnOnce(&mut LocalContextGuard) -> T) -> Result<T> {
        let world = self.snap.world;
        let Some(..) = world.main_id() else {
            log::error!("Project: main file is not set");
            bail!("main file is not set");
        };

        let mut analysis = self.analysis.snapshot_(world, self.rev_lock);
        Ok(f(&mut analysis))
    }
}
