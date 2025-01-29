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
//! running compiler with callbacking [`CompileHandlerImpl`] incrementally. An
//! additional [`ProjectState`] is also created to control the
//! [`ProjectCompiler`].
//!
//! The [`CompileHandlerImpl`] will push information to other actors.

#![allow(missing_docs)]

pub use tinymist_project::*;

use std::sync::Arc;

use log::{error, trace};
use parking_lot::Mutex;
use reflexo::{hash::FxHashMap, path::unix_slash};
use reflexo_typst::CompileReport;
use sync_lsp::LspClient;
use tinymist_query::{
    analysis::{Analysis, AnalysisRevLock, LocalContextGuard},
    CompilerQueryRequest, CompilerQueryResponse, DiagnosticsMap, SemanticRequest, StatefulRequest,
    VersionedDocument,
};
use tinymist_std::{bail, error::prelude::*};
use tokio::sync::mpsc;

use crate::actor::editor::{CompileStatus, DocVersion, EditorRequest, TinymistCompileStatusEnum};
use crate::stats::{CompilerQueryStats, QueryStatGuard};

type EditorSender = mpsc::UnboundedSender<EditorRequest>;

/// LSP project compiler.
pub type LspProjectCompiler = ProjectCompiler<LspCompilerFeat, ProjectInsStateExt>;

#[derive(Default, Clone)]
pub struct ProjectPreviewState {
    #[cfg(feature = "preview")]
    pub(crate) inner: Arc<Mutex<FxHashMap<ProjectInsId, Arc<typst_preview::CompileWatcher>>>>,
}

#[cfg(feature = "preview")]
impl ProjectPreviewState {
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
pub struct ProjectInsStateExt {
    pub is_compiling: bool,
    pub last_compilation: Option<LspCompiledArtifact>,
}

pub struct ProjectState {
    pub state: LspProjectCompiler,
    pub preview: ProjectPreviewState,
    pub analysis: Arc<Analysis>,
    pub stats: CompilerQueryStats,
    pub export: crate::task::ExportTask,
}

impl ProjectState {
    /// Snapshot the compiler thread for tasks
    pub fn snapshot(&mut self) -> Result<LspCompileSnapshot> {
        Ok(self.state.snapshot())
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

    pub fn interrupt(&mut self, intr: Interrupt<LspCompilerFeat>) {
        if let Interrupt::Compiled(compiled) = &intr {
            let proj = self.state.projects().find(|p| p.id == compiled.id);
            if let Some(proj) = proj {
                proj.ext.is_compiling = false;
                proj.ext.last_compilation = Some(compiled.clone());
            }
        }

        self.state.process(intr);
    }

    pub(crate) fn stop(&mut self) {
        // todo: stop all compilations
    }

    pub(crate) fn restart_dedicate(
        &mut self,
        group: &str,
        entry: EntryState,
    ) -> Result<ProjectInsId> {
        self.state.restart_dedicate(group, entry)
    }
}

pub struct CompileHandlerImpl {
    pub(crate) analysis: Arc<Analysis>,

    pub(crate) preview: ProjectPreviewState,

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
    fn push_diagnostics(&self, dv: DocVersion, diagnostics: Option<DiagnosticsMap>) {
        let res = self.editor_tx.send(EditorRequest::Diag(dv, diagnostics));
        if let Err(err) = res {
            error!("failed to send diagnostics: {err:#}");
        }
    }

    fn notify_diagnostics(&self, snap: &LspCompiledArtifact) {
        let world = &snap.world;
        let dv = DocVersion {
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

            trace!("notify diagnostics({dv:?}): {diagnostics:#?}");
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
                log::info!("Project: skip compilation for {id:?} due to harmless vfs changes");
                s.reason = CompileReasons::default();
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
                let dv = DocVersion {
                    id: id.clone(),
                    revision,
                };
                self.push_diagnostics(dv, None);
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

    fn notify_compile(&self, snap: &LspCompiledArtifact, rep: CompileReport) {
        // todo: we need to manage the revision for fn status() as well
        {
            let mut n_rev = self.notified_revision.lock();
            if *n_rev >= snap.world.revision().get() {
                log::info!(
                    "Project: already notified for revision {} <= {n_rev}",
                    snap.world.revision(),
                );
                return;
            }
            *n_rev = snap.world.revision().get();
        }

        self.notify_diagnostics(snap);

        self.client.send_event(LspInterrupt::Compiled(snap.clone()));
        self.export.signal(snap);

        self.editor_tx
            .send(EditorRequest::Status(CompileStatus {
                id: snap.id.clone(),
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
            error!("Project: main file is not set");
            bail!("main file is not set");
        };

        let mut analysis = self.analysis.snapshot_(world, self.rev_lock);
        Ok(f(&mut analysis))
    }
}
