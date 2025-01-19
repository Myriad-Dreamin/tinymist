//! The project.
//!
//! ```ascii
//! ┌────────────────────────────────┐         ┌────────────┐
//! │      main::compile_actor       │◄───────►│notify_actor│
//! └─────┬────────────────────▲─────┘         └────────────┘
//!       │                    │
//! ┌─────▼────────────────────┴─────┐ handler ┌────────────┐
//! │   compiler::compile_handler    ├────────►│ rest actors│
//! └────────────────────────────────┘         └────────────┘
//! ```
//!
//! We use typst by creating a [`ProjectCompiler`] and
//! running compiler with callbacking [`LspProjectHandler`] incrementally. An
//! additional [`LocalCompileHandler`] is also created to control the
//! [`ProjectCompiler`].
//!
//! The [`LspProjectHandler`] will push information to other actors.

#![allow(missing_docs)]

pub use tinymist_project::*;

use std::sync::Arc;

use anyhow::bail;
use log::{error, info, trace};
use parking_lot::Mutex;
use reflexo::{hash::FxHashMap, path::unix_slash};
use reflexo_typst::{typst::prelude::EcoVec, CompileReport};
use sync_lsp::LspClient;
use tinymist_query::{
    analysis::{Analysis, AnalysisRevLock, LocalContextGuard},
    CompilerQueryRequest, CompilerQueryResponse, DiagnosticsMap, SemanticRequest, StatefulRequest,
    VersionedDocument,
};
use tinymist_std::error::prelude::*;
use tokio::sync::{mpsc, oneshot};
use typst::{diag::SourceDiagnostic, World};

use crate::actor::editor::{CompileStatus, DocVersion, EditorRequest, TinymistCompileStatusEnum};
use crate::stats::{CompilerQueryStats, QueryStatGuard};
use crate::world::vfs::MemoryEvent;

type EditorSender = mpsc::UnboundedSender<EditorRequest>;

#[derive(Default, Clone)]
pub struct LspPreviewState {
    #[cfg(feature = "preview")]
    pub(crate) inner: Arc<Mutex<FxHashMap<ProjectInsId, Arc<typst_preview::CompileWatcher>>>>,
}

#[cfg(feature = "preview")]
impl LspPreviewState {
    // todo: multiple preview support
    #[must_use]
    pub fn register(&self, id: &ProjectInsId, handle: &Arc<typst_preview::CompileWatcher>) -> bool {
        let mut p = self.inner.lock();
        if p.contains_key(id) {
            return false;
        }

        p.insert(id.clone(), handle.clone());
        true
    }

    #[must_use]
    pub fn unregister(&self, task_id: &ProjectInsId) -> bool {
        let mut p = self.inner.lock();
        if p.remove(task_id).is_some() {
            return true;
        }

        false
    }

    #[must_use]
    pub fn get(&self, task_id: &ProjectInsId) -> Option<Arc<typst_preview::CompileWatcher>> {
        self.inner.lock().get(task_id).cloned()
    }
}

#[derive(Default)]
pub struct ProjectStateExt {
    pub is_compiling: bool,
}

/// LSP project compiler.
pub type LspProjectCompiler = ProjectCompiler<LspCompilerFeat, ProjectStateExt>;

pub struct Project {
    pub diag_group: String,
    pub state: LspProjectCompiler,
    pub preview: LspPreviewState,
    pub analysis: Arc<Analysis>,
    pub stats: CompilerQueryStats,
    pub export: crate::task::ExportTask,
}

impl Project {
    /// Snapshot the compiler thread for tasks
    pub fn snapshot(&mut self) -> ZResult<WorldSnapFut> {
        let (tx, rx) = oneshot::channel();
        let snap = self.state.snapshot();
        let _ = tx.send(snap);

        Ok(WorldSnapFut { rx })
    }

    /// Snapshot the compiler thread for language queries
    pub fn query_snapshot(&mut self, q: Option<&CompilerQueryRequest>) -> ZResult<QuerySnapFut> {
        let fut = self.snapshot()?;
        let analysis = self.analysis.clone();
        let rev_lock = analysis.lock_revision(q);

        Ok(QuerySnapFut {
            fut,
            analysis,
            rev_lock,
        })
    }

    pub fn add_memory_changes(&mut self, event: MemoryEvent) {
        self.state.process(Interrupt::Memory(event));
    }

    pub fn interrupt(&mut self, intr: Interrupt<LspCompilerFeat>) {
        if let Interrupt::Compiled(compiled) = &intr {
            let proj = self.state.projects().find(|p| p.id == compiled.id);
            if let Some(proj) = proj {
                proj.ext.is_compiling = false;
            }
        }

        self.state.process(intr);
    }

    pub fn change_task(&mut self, task: TaskInputs) {
        self.state
            .process(Interrupt::ChangeTask(self.state.primary.id.clone(), task));
    }

    pub(crate) fn stop(&mut self) {
        // todo: stop all compilations
    }

    pub(crate) fn restart_dedicate(
        &mut self,
        group: &str,
        entry: EntryState,
    ) -> ZResult<ProjectInsId> {
        self.state.restart_dedicate(group, entry)
    }
}

pub struct CompileHandlerImpl {
    pub(crate) diag_group: String,
    pub(crate) analysis: Arc<Analysis>,

    pub(crate) preview: LspPreviewState,

    pub(crate) export: crate::task::ExportTask,
    pub(crate) editor_tx: EditorSender,
    pub(crate) client: Box<dyn ProjectClient>,

    pub(crate) notified_revision: parking_lot::Mutex<usize>,
}

pub trait ProjectClient: Send + Sync + 'static {
    fn send_event(&self, event: LspInterrupt);
}

impl ProjectClient for LspClient {
    fn send_event(&self, event: LspInterrupt) {
        self.send_event(event);
    }
}

impl ProjectClient for tokio::sync::mpsc::UnboundedSender<LspInterrupt> {
    fn send_event(&self, event: LspInterrupt) {
        if let Err(err) = self.send(event) {
            error!("failed to send interrupt: {err}");
        }
    }
}

impl CompileHandlerImpl {
    fn push_diagnostics(&self, revision: usize, diagnostics: Option<DiagnosticsMap>) {
        let dv = DocVersion {
            group: self.diag_group.clone(),
            revision,
        };
        let res = self.editor_tx.send(EditorRequest::Diag(dv, diagnostics));
        if let Err(err) = res {
            error!("failed to send diagnostics: {err:#}");
        }
    }

    fn notify_diagnostics(
        &self,
        world: &LspWorld,
        errors: EcoVec<SourceDiagnostic>,
        warnings: EcoVec<SourceDiagnostic>,
    ) {
        let revision = world.revision().get();
        trace!("notify diagnostics({revision}): {errors:#?} {warnings:#?}");

        let diagnostics = tinymist_query::convert_diagnostics(
            world,
            errors.iter().chain(warnings.iter()),
            self.analysis.position_encoding,
        );

        let entry = world.entry_state();
        // todo: better way to remove diagnostics
        // todo: check all errors in this file
        let detached = entry.is_inactive();
        let valid = !detached;
        self.push_diagnostics(revision, valid.then_some(diagnostics));
    }
}

impl CompileHandler<LspCompilerFeat, ProjectStateExt> for CompileHandlerImpl {
    fn on_any_compile_reason(&self, c: &mut LspProjectCompiler) {
        let instances_mut = std::iter::once(&mut c.primary).chain(c.dedicates.iter_mut());
        for s in instances_mut {
            if s.ext.is_compiling {
                continue;
            }

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
        // todo: seems to duplicate with CompileStatus
        let status = match rep {
            CompileReport::Suspend => {
                self.push_diagnostics(revision, None);
                TinymistCompileStatusEnum::CompileSuccess
            }
            CompileReport::Stage(_, _, _) => TinymistCompileStatusEnum::Compiling,
            CompileReport::CompileSuccess(_, _, _) => TinymistCompileStatusEnum::CompileSuccess,
            CompileReport::CompileError(_, _, _) | CompileReport::ExportError(_, _, _) => {
                TinymistCompileStatusEnum::CompileError
            }
        };

        let this = &self;
        this.editor_tx
            .send(EditorRequest::Status(CompileStatus {
                group: this.diag_group.clone(),
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

    fn notify_compile(&self, snap: &CompiledArtifact<LspCompilerFeat>, rep: CompileReport) {
        // todo: we need to manage the revision for fn status() as well
        {
            let mut n_rev = self.notified_revision.lock();
            if *n_rev >= snap.world.revision().get() {
                log::info!(
                    "TypstActor: already notified for revision {} <= {n_rev}",
                    snap.world.revision(),
                );
                return;
            }
            *n_rev = snap.world.revision().get();
        }

        self.notify_diagnostics(
            &snap.world,
            snap.doc.clone().err().unwrap_or_default(),
            snap.warnings.clone(),
        );

        self.client.send_event(LspInterrupt::Compiled(snap.clone()));
        self.export.signal(snap, snap.signal);

        self.editor_tx
            .send(EditorRequest::Status(CompileStatus {
                group: self.diag_group.clone(),
                path: rep
                    .compiling_id()
                    .map(|s| unix_slash(s.vpath().as_rooted_path()))
                    .unwrap_or_default(),
                status: if snap.doc.is_ok() {
                    TinymistCompileStatusEnum::CompileSuccess
                } else {
                    TinymistCompileStatusEnum::CompileError
                },
            }))
            .unwrap();

        #[cfg(feature = "preview")]
        if let Some(inner) = self.preview.get(&snap.id) {
            let snap = snap.clone();
            inner.notify_compile(Arc::new(crate::tool::preview::PreviewCompileView { snap }));
        }
    }
}

pub struct QuerySnapWithStat {
    pub fut: QuerySnapFut,
    pub(crate) stat: QueryStatGuard,
}

pub struct WorldSnapFut {
    rx: oneshot::Receiver<CompileSnapshot<LspCompilerFeat>>,
}

impl WorldSnapFut {
    /// wait for the snapshot to be ready
    pub async fn receive(self) -> ZResult<CompileSnapshot<LspCompilerFeat>> {
        self.rx
            .await
            .map_err(map_string_err("failed to get snapshot"))
    }
}

pub struct QuerySnapFut {
    fut: WorldSnapFut,
    analysis: Arc<Analysis>,
    rev_lock: AnalysisRevLock,
}

impl QuerySnapFut {
    /// wait for the snapshot to be ready
    pub async fn receive(self) -> ZResult<QuerySnap> {
        let snap = self.fut.receive().await?;
        Ok(QuerySnap {
            snap,
            analysis: self.analysis,
            rev_lock: self.rev_lock,
        })
    }
}

pub struct QuerySnap {
    pub snap: CompileSnapshot<LspCompilerFeat>,
    analysis: Arc<Analysis>,
    rev_lock: AnalysisRevLock,
}

impl std::ops::Deref for QuerySnap {
    type Target = CompileSnapshot<LspCompilerFeat>;

    fn deref(&self) -> &Self::Target {
        &self.snap
    }
}

impl QuerySnap {
    pub fn task(mut self, inputs: TaskInputs) -> Self {
        self.snap = self.snap.task(inputs);
        self
    }

    pub fn run_stateful<T: StatefulRequest>(
        self,
        query: T,
        wrapper: fn(Option<T::Response>) -> CompilerQueryResponse,
    ) -> anyhow::Result<CompilerQueryResponse> {
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
    ) -> anyhow::Result<CompilerQueryResponse> {
        self.run_analysis(|ctx| query.request(ctx)).map(wrapper)
    }

    pub fn run_analysis<T>(self, f: impl FnOnce(&mut LocalContextGuard) -> T) -> anyhow::Result<T> {
        let world = self.snap.world;
        let Some(main) = world.main_id() else {
            error!("TypstActor: main file is not set");
            bail!("main file is not set");
        };
        world.source(main).map_err(|err| {
            info!("TypstActor: failed to prepare main file: {err:?}");
            anyhow::anyhow!("failed to get source: {err}")
        })?;

        let mut analysis = self.analysis.snapshot_(world, self.rev_lock);
        Ok(f(&mut analysis))
    }
}
