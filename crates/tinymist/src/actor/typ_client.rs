//! The actor that runs compilations.
//!
//! ```ascii
//! ┌────────────────────────────────┐
//! │  main::compile_actor (client)  │
//! └─────┬────────────────────▲─────┘
//!       │                    │
//! ┌─────▼────────────────────┴─────┐         ┌────────────┐
//! │compiler::compile_actor (server)│◄───────►│notify_actor│
//! └─────┬────────────────────▲─────┘         └────────────┘
//!       │                    │
//! ┌─────▼────────────────────┴─────┐ handler ┌────────────┐
//! │   compiler::compile_handler    ├────────►│ rest actors│
//! └────────────────────────────────┘         └────────────┘
//! ```
//!
//! We use typst by creating a
//! [`CompileServerActor`][`crate::actor::typ_server::CompileServerActor`] and
//! running compiler with callbacking [`CompileHandler`] incrementally. An
//! additional [`CompileClientActor`] is also created to control the
//! [`CompileServerActor`][`crate::actor::typ_server::CompileServerActor`].
//!
//! The [`CompileHandler`] will push information to other actors.

use std::{collections::HashMap, ops::Deref, sync::Arc};

use anyhow::bail;
use log::{error, info, trace};
use reflexo_typst::{
    error::prelude::*, typst::prelude::*, vfs::notify::MemoryEvent, world::EntryState,
    CompileReport, EntryReader, Error, ImmutPath, TaskInputs,
};
use sync_lsp::{just_future, QueryFuture};
use tinymist_query::{
    analysis::{Analysis, AnalysisRevLock, LocalContextGuard},
    CompilerQueryRequest, CompilerQueryResponse, DiagnosticsMap, OnExportRequest, SemanticRequest,
    ServerInfoResponse, StatefulRequest, VersionedDocument,
};
use tokio::sync::{mpsc, oneshot};
use typst::{diag::SourceDiagnostic, World};

use super::{
    editor::{DocVersion, EditorRequest, TinymistCompileStatusEnum},
    typ_server::{
        CompilationHandle, CompileSnapshot, CompiledArtifact, Interrupt, SucceededArtifact,
    },
};
use crate::{
    stats::{CompilerQueryStats, QueryStatGuard},
    task::{ExportTask, ExportUserConfig},
    world::{LspCompilerFeat, LspWorld},
    CompileConfig,
};

type EditorSender = mpsc::UnboundedSender<EditorRequest>;

pub struct CompileHandler {
    pub(crate) diag_group: String,
    pub(crate) analysis: Arc<Analysis>,
    pub(crate) stats: CompilerQueryStats,

    #[cfg(feature = "preview")]
    pub(crate) inner: Arc<parking_lot::RwLock<Option<Arc<typst_preview::CompileWatcher>>>>,

    pub(crate) intr_tx: mpsc::UnboundedSender<Interrupt<LspCompilerFeat>>,
    pub(crate) export: ExportTask,
    pub(crate) editor_tx: EditorSender,

    pub(crate) notified_revision: parking_lot::Mutex<usize>,
}

impl CompileHandler {
    /// Snapshot the compiler thread for tasks
    pub fn snapshot(&self) -> ZResult<WorldSnapFut> {
        let (tx, rx) = oneshot::channel();
        self.intr_tx
            .send(Interrupt::SnapshotRead(tx))
            .map_err(map_string_err("failed to send snapshot request"))?;

        Ok(WorldSnapFut { rx })
    }

    /// Snapshot the compiler thread for language queries
    pub fn query_snapshot(&self, q: Option<&CompilerQueryRequest>) -> ZResult<QuerySnapFut> {
        let fut = self.snapshot()?;
        let analysis = self.analysis.clone();
        let rev_lock = analysis.lock_revision(q);

        Ok(QuerySnapFut {
            fut,
            analysis,
            rev_lock,
        })
    }

    /// Get latest artifact the compiler thread for tasks
    pub fn artifact(&self) -> ZResult<ArtifactSnap> {
        let (tx, rx) = oneshot::channel();
        self.intr_tx
            .send(Interrupt::CurrentRead(tx))
            .map_err(map_string_err("failed to send snapshot request"))?;

        Ok(ArtifactSnap { rx })
    }

    pub fn flush_compile(&self) {
        // todo: better way to flush compile
        let _ = self.intr_tx.send(Interrupt::Compile);
    }

    pub fn add_memory_changes(&self, event: MemoryEvent) {
        let _ = self.intr_tx.send(Interrupt::Memory(event));
    }

    pub fn change_task(&self, task_inputs: TaskInputs) {
        let _ = self.intr_tx.send(Interrupt::ChangeTask(task_inputs));
    }

    pub async fn settle(&self) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        let _ = self.intr_tx.send(Interrupt::Settle(tx));
        rx.await?;
        Ok(())
    }

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

    // todo: multiple preview support
    #[cfg(feature = "preview")]
    #[must_use]
    pub fn register_preview(&self, handle: &Arc<typst_preview::CompileWatcher>) -> bool {
        let mut p = self.inner.write();
        if p.as_ref().is_some() {
            return false;
        }
        *p = Some(handle.clone());
        true
    }

    #[cfg(feature = "preview")]
    #[must_use]
    pub fn unregister_preview(&self, task_id: &str) -> bool {
        let mut p = self.inner.write();
        if p.as_ref().is_some_and(|p| p.task_id() == task_id) {
            *p = None;
            return true;
        }
        false
    }
}

impl CompilationHandle<LspCompilerFeat> for CompileHandler {
    fn status(&self, revision: usize, _rep: CompileReport) {
        // todo: seems to duplicate with CompileStatus
        let status = match _rep {
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
            .send(EditorRequest::Status(this.diag_group.clone(), status))
            .unwrap();

        #[cfg(feature = "preview")]
        if let Some(inner) = this.inner.read().as_ref() {
            use typst_preview::CompileStatus;

            let status = match _rep {
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

    fn notify_compile(&self, snap: &CompiledArtifact<LspCompilerFeat>, _rep: CompileReport) {
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

        self.export.signal(snap, snap.signal);

        self.editor_tx
            .send(EditorRequest::Status(
                self.diag_group.clone(),
                if snap.doc.is_ok() {
                    TinymistCompileStatusEnum::CompileSuccess
                } else {
                    TinymistCompileStatusEnum::CompileError
                },
            ))
            .unwrap();

        #[cfg(feature = "preview")]
        if let Some(inner) = self.inner.read().as_ref() {
            let res = snap
                .doc
                .clone()
                .map_err(|_| typst_preview::CompileStatus::CompileError);
            inner.notify_compile(res, snap.signal.by_fs_events, snap.signal.by_entry_update);
        }
    }
}

pub struct CompileClientActor {
    pub handle: Arc<CompileHandler>,

    pub config: CompileConfig,
    entry: EntryState,
}

impl CompileClientActor {
    pub(crate) fn new(
        handle: Arc<CompileHandler>,
        config: CompileConfig,
        entry: EntryState,
    ) -> Self {
        Self {
            handle,
            config,
            entry,
        }
    }

    /// Snapshot the compiler thread for tasks
    pub fn snapshot(&self) -> ZResult<WorldSnapFut> {
        self.handle.clone().snapshot()
    }

    /// Snapshot the compiler thread for language queries
    pub fn query_snapshot(&self) -> ZResult<QuerySnapFut> {
        self.handle.clone().query_snapshot(None)
    }

    /// Snapshot the compiler thread for language queries
    pub fn query_snapshot_with_stat(&self, q: &CompilerQueryRequest) -> ZResult<QuerySnapWithStat> {
        let name: &'static str = q.into();
        let path = q.associated_path();
        let stat = self.handle.stats.query_stat(path, name);
        let fut = self.handle.clone().query_snapshot(Some(q))?;
        Ok(QuerySnapWithStat { fut, stat })
    }

    pub fn add_memory_changes(&self, event: MemoryEvent) {
        self.handle.add_memory_changes(event);
    }

    pub fn change_task(&self, task_inputs: TaskInputs) {
        self.handle.change_task(task_inputs);
    }

    pub fn sync_config(&mut self, config: CompileConfig) {
        self.config = config;
    }

    pub(crate) fn change_export_config(&mut self, config: ExportUserConfig) {
        self.handle.export.change_config(config);
    }

    pub fn on_export(&self, req: OnExportRequest) -> QueryFuture {
        let OnExportRequest { path, kind, open } = req;
        let snap = self.snapshot()?;

        let entry = self.config.determine_entry(Some(path.as_path().into()));
        let export = self.handle.export.oneshot(snap, Some(entry), kind);
        just_future(async move {
            let res = export.await?;

            if let Some(Some(path)) = open.then_some(res.as_ref()) {
                log::info!("open with system default apps: {path:?}");
                if let Err(e) = ::open::that_detached(path) {
                    log::error!("failed to open with system default apps: {e}");
                };
            }

            log::info!("CompileActor: on export end: {path:?} as {res:?}");
            Ok(tinymist_query::CompilerQueryResponse::OnExport(res))
        })
    }
}

impl CompileClientActor {
    pub async fn settle(&mut self) {
        let _ = self.change_entry(None);
        info!("TypstActor({}): settle requested", self.handle.diag_group);
        match self.handle.settle().await {
            Ok(()) => info!("TypstActor({}): settled", self.handle.diag_group),
            Err(err) => error!(
                "TypstActor({}): failed to settle: {err:#}",
                self.handle.diag_group
            ),
        }
    }

    pub fn change_entry(&mut self, path: Option<ImmutPath>) -> Result<bool, Error> {
        if path
            .as_deref()
            .is_some_and(|p| !p.is_absolute() && !p.starts_with("/untitled"))
        {
            return Err(error_once!("entry file must be absolute", path: path.unwrap().display()));
        }

        let next_entry = self.config.determine_entry(path);
        if next_entry == self.entry {
            return Ok(false);
        }

        let diag_group = &self.handle.diag_group;
        info!("the entry file of TypstActor({diag_group}) is changing to {next_entry:?}");

        self.change_task(TaskInputs {
            entry: Some(next_entry.clone()),
            ..Default::default()
        });

        self.entry = next_entry;

        Ok(true)
    }

    pub fn clear_cache(&mut self) {
        self.handle.analysis.clear_cache();
    }

    pub fn collect_server_info(&self) -> QueryFuture {
        let dg = self.handle.diag_group.clone();
        let api_stats = self.handle.stats.report();
        let query_stats = self.handle.analysis.report_query_stats();
        let alloc_stats = self.handle.analysis.report_alloc_stats();

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
        let w = self.world.as_ref();
        let Some(main) = w.main_id() else {
            error!("TypstActor: main file is not set");
            bail!("main file is not set");
        };
        w.source(main).map_err(|err| {
            info!("TypstActor: failed to prepare main file: {err:?}");
            anyhow::anyhow!("failed to get source: {err}")
        })?;

        let mut analysis = self.analysis.snapshot_(w.clone(), self.rev_lock);
        Ok(f(&mut analysis))
    }
}

pub struct ArtifactSnap {
    rx: oneshot::Receiver<SucceededArtifact<LspCompilerFeat>>,
}

impl ArtifactSnap {
    /// Get latest artifact the compiler thread for tasks
    pub async fn receive(self) -> ZResult<SucceededArtifact<LspCompilerFeat>> {
        self.rx
            .await
            .map_err(map_string_err("failed to get snapshot"))
    }
}
