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
//! │    compiler::compile_driver    ├────────►│ rest actors│
//! └────────────────────────────────┘         └────────────┘
//! ```
//!
//! We generally use typst in two ways.
//! + creates a [`CompileDriver`] and run compilation in fly.
//! + creates a [`CompileServerActor`], wraps the driver, and runs
//!   [`CompileDriver`] incrementally.
//!
//! For latter case, an additional [`CompileClientActor`] is created to
//! control the [`CompileServerActor`].
//!
//! The [`CompileDriver`] will also keep a [`CompileHandler`] to push
//! information to other actors.

use std::{
    collections::HashMap,
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{anyhow, bail};
use log::{error, info, trace};
use parking_lot::Mutex;
use tinymist_query::{
    analysis::{Analysis, AnalysisContext, AnalysisResources},
    DiagnosticsMap, ExportKind, ServerInfoResponse, VersionedDocument,
};
use tinymist_render::PeriscopeRenderer;
use tokio::sync::{mpsc, oneshot, watch};
use typst::{
    diag::{PackageError, SourceDiagnostic},
    layout::Position,
    model::Document as TypstDocument,
    syntax::package::PackageSpec,
    World as TypstWorld,
};
use typst_ts_compiler::{vfs::notify::MemoryEvent, CompileReport, EntryReader, TaskInputs};
use typst_ts_core::{
    config::compiler::EntryState, debug_loc::DataSource, error::prelude::*, typst::prelude::EcoVec,
    Error, ImmutPath, TypstFont,
};

use super::{
    editor::{EditorRequest, TinymistCompileStatusEnum},
    export::ExportConfig,
    typ_server::{CompilationHandle, CompileSnapshot, CompiledArtifact, Interrupt},
};
use crate::{
    actor::export::ExportRequest,
    compile_init::CompileConfig,
    tools::preview::CompileStatus,
    utils::{self, threaded_receive},
    world::{LspCompilerFeat, LspWorld},
};

type EditorSender = mpsc::UnboundedSender<EditorRequest>;

use crate::tools::preview::CompilationHandle as PreviewCompilationHandle;

pub struct CompileHandler {
    pub(crate) diag_group: String,
    pub(crate) analysis: Analysis,
    pub(crate) periscope: PeriscopeRenderer,

    #[cfg(feature = "preview")]
    pub(crate) inner: Arc<Option<typst_preview::CompilationHandleImpl>>,

    pub(crate) doc_tx: watch::Sender<Option<Arc<TypstDocument>>>,
    pub(crate) export_tx: mpsc::UnboundedSender<ExportRequest>,
    pub(crate) editor_tx: EditorSender,
}

impl PreviewCompilationHandle for CompileHandler {
    fn status(&self, _status: CompileStatus) {
        self.editor_tx
            .send(EditorRequest::Status(
                self.diag_group.clone(),
                TinymistCompileStatusEnum::Compiling,
            ))
            .unwrap();

        #[cfg(feature = "preview")]
        if let Some(inner) = self.inner.as_ref() {
            inner.status(_status);
        }
    }

    fn notify_compile(&self, res: Result<Arc<TypstDocument>, CompileStatus>) {
        if let Ok(doc) = res.clone() {
            let _ = self.doc_tx.send(Some(doc.clone()));
            let _ = self.export_tx.send(ExportRequest::OnTyped);
        }

        self.editor_tx
            .send(EditorRequest::Status(
                self.diag_group.clone(),
                if res.is_ok() {
                    TinymistCompileStatusEnum::CompileSuccess
                } else {
                    TinymistCompileStatusEnum::CompileError
                },
            ))
            .unwrap();

        #[cfg(feature = "preview")]
        if let Some(inner) = self.inner.as_ref() {
            inner.notify_compile(res);
        }
    }
}

impl CompilationHandle<LspCompilerFeat> for CompileHandler {
    fn status(&self, rep: CompileReport) {
        let status = match rep {
            CompileReport::Suspend => {
                self.push_diagnostics(None);
                CompileStatus::CompileError
            }
            CompileReport::Stage(_, _, _) => CompileStatus::Compiling,
            CompileReport::CompileSuccess(_, _, _) | CompileReport::CompileWarning(_, _, _) => {
                CompileStatus::CompileSuccess
            }
            CompileReport::CompileError(_, _, _) | CompileReport::ExportError(_, _, _) => {
                CompileStatus::CompileError
            }
        };

        <Self as PreviewCompilationHandle>::status(self, status);
    }

    fn notify_compile(&self, snap: &CompiledArtifact<LspCompilerFeat>, _rep: CompileReport) {
        let (res, err) = match snap.doc.clone() {
            Ok(doc) => (Ok(doc), EcoVec::new()),
            Err(err) => (Err(CompileStatus::CompileError), err),
        };
        self.notify_diagnostics(
            &snap.world,
            err,
            snap.env.tracer.as_ref().map(|e| e.clone().warnings()),
        );

        <Self as PreviewCompilationHandle>::notify_compile(self, res);
    }
}

impl CompileHandler {
    fn push_diagnostics(&self, diagnostics: Option<DiagnosticsMap>) {
        let res = self
            .editor_tx
            .send(EditorRequest::Diag(self.diag_group.clone(), diagnostics));
        if let Err(err) = res {
            error!("failed to send diagnostics: {err:#}");
        }
    }

    fn notify_diagnostics(
        &self,
        world: &LspWorld,
        errors: EcoVec<SourceDiagnostic>,
        warnings: Option<EcoVec<SourceDiagnostic>>,
    ) {
        trace!("notify diagnostics: {errors:#?} {warnings:#?}");

        let diagnostics = self.run_analysis(world, |ctx| {
            tinymist_query::convert_diagnostics(ctx, errors.iter().chain(warnings.iter().flatten()))
        });

        match diagnostics {
            Ok(diagnostics) => {
                let entry = world.entry_state();
                // todo: better way to remove diagnostics
                // todo: check all errors in this file
                let detached = entry.is_inactive();
                let valid = !detached;
                self.push_diagnostics(valid.then_some(diagnostics));
            }
            Err(err) => {
                error!("TypstActor: failed to convert diagnostics: {:#}", err);
                self.push_diagnostics(None);
            }
        }
    }

    pub fn run_analysis<T>(
        &self,
        w: &LspWorld,
        f: impl FnOnce(&mut AnalysisContext<'_>) -> T,
    ) -> anyhow::Result<T> {
        let Some(main) = w.main_id() else {
            error!("TypstActor: main file is not set");
            bail!("main file is not set");
        };
        let Some(root) = w.entry_state().root() else {
            error!("TypstActor: root is not set");
            bail!("root is not set");
        };
        w.source(main).map_err(|err| {
            info!("TypstActor: failed to prepare main file: {err:?}");
            anyhow!("failed to get source: {err}")
        })?;

        struct WrapWorld<'a>(&'a LspWorld, &'a PeriscopeRenderer);

        impl<'a> AnalysisResources for WrapWorld<'a> {
            fn world(&self) -> &dyn typst::World {
                self.0
            }

            fn resolve(&self, spec: &PackageSpec) -> Result<Arc<Path>, PackageError> {
                use typst_ts_compiler::package::Registry;
                self.0.registry.resolve(spec)
            }

            fn iter_dependencies(&self, f: &mut dyn FnMut(ImmutPath)) {
                use typst_ts_compiler::WorldDeps;
                self.0.iter_dependencies(f)
            }

            /// Resolve extra font information.
            fn font_info(&self, font: TypstFont) -> Option<Arc<DataSource>> {
                self.0.font_resolver.describe_font(&font)
            }

            /// Resolve periscope image at the given position.
            fn periscope_at(
                &self,
                ctx: &mut AnalysisContext,
                doc: VersionedDocument,
                pos: Position,
            ) -> Option<String> {
                self.1.render_marked(ctx, doc, pos)
            }
        }

        let w = WrapWorld(w, &self.periscope);

        let mut analysis = self.analysis.snapshot(root, &w);
        Ok(f(&mut analysis))
    }
}

pub struct CompileClientActor {
    pub handle: Arc<CompileHandler>,

    pub config: CompileConfig,
    entry: EntryState,
    intr_tx: mpsc::UnboundedSender<Interrupt<LspCompilerFeat>>,
}

impl CompileClientActor {
    pub(crate) fn new(
        handle: Arc<CompileHandler>,
        config: CompileConfig,
        entry: EntryState,
        intr_tx: mpsc::UnboundedSender<Interrupt<LspCompilerFeat>>,
    ) -> Self {
        Self {
            handle,
            config,
            entry,
            intr_tx,
        }
    }

    /// Snapshot the compiler thread for tasks
    pub fn snapshot(&self) -> ZResult<QuerySnap> {
        let (tx, rx) = oneshot::channel();
        self.intr_tx
            .send(Interrupt::Snapshot(tx))
            .map_err(map_string_err("failed to send snapshot request"))?;

        Ok(QuerySnap {
            rx: Arc::new(Mutex::new(Some(rx))),
            snap: tokio::sync::OnceCell::new(),
            handle: self.handle.clone(),
        })
    }

    /// Snapshot the compiler thread for tasks
    pub fn sync_snapshot(&self) -> ZResult<CompileSnapshot<LspCompilerFeat>> {
        let (tx, rx) = oneshot::channel();

        self.intr_tx
            .send(Interrupt::Snapshot(tx))
            .map_err(map_string_err("failed to send snapshot request"))?;

        threaded_receive(rx).map_err(map_string_err("failed to get snapshot"))
    }

    pub fn sync_config(&mut self, config: CompileConfig) {
        self.config = config;
    }

    pub fn add_memory_changes(&self, event: MemoryEvent) {
        let _ = self.intr_tx.send(Interrupt::Memory(event));
    }

    pub fn change_task(&self, task_inputs: TaskInputs) {
        let _ = self.intr_tx.send(Interrupt::ChangeTask(task_inputs));
    }

    pub(crate) fn change_export_pdf(&mut self, config: ExportConfig) {
        let _ = self
            .handle
            .export_tx
            .send(ExportRequest::ChangeConfig(config));
    }

    pub fn on_export(&self, kind: ExportKind, path: PathBuf) -> anyhow::Result<Option<PathBuf>> {
        // todo: we currently doesn't respect the path argument...
        info!("CompileActor: on export: {}", path.display());

        let (tx, rx) = oneshot::channel();
        let _ = self
            .handle
            .export_tx
            .send(ExportRequest::Oneshot(Some(kind), tx));
        let res: Option<PathBuf> = utils::threaded_receive(rx)?;

        info!("CompileActor: on export end: {path:?} as {res:?}");
        Ok(res)
    }

    pub fn on_save_export(&self, path: PathBuf) -> anyhow::Result<()> {
        info!("CompileActor: on save export: {}", path.display());
        let _ = self.handle.export_tx.send(ExportRequest::OnSaved);

        Ok(())
    }
}

impl CompileClientActor {
    pub fn settle(&mut self) {
        let _ = self.change_entry(None);
        info!("TypstActor({}): settle requested", self.handle.diag_group);
        let (tx, rx) = oneshot::channel();
        let _ = self.intr_tx.send(Interrupt::Settle(tx));
        match utils::threaded_receive(rx) {
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
        // todo: let export request accept compiled artifact
        let _ = self
            .handle
            .export_tx
            .send(ExportRequest::ChangeExportPath(next_entry.clone()));

        self.entry = next_entry;

        Ok(true)
    }

    pub fn clear_cache(&self) {
        self.handle.analysis.clear_cache();
    }

    pub fn collect_server_info(&self) -> anyhow::Result<HashMap<String, ServerInfoResponse>> {
        let dg = self.handle.diag_group.clone();

        let snap = self.sync_snapshot()?;
        let w = &snap.world;

        let info = ServerInfoResponse {
            root: w.entry_state().root().map(|e| e.as_ref().to_owned()),
            font_paths: w.font_resolver.font_paths().to_owned(),
            inputs: w.inputs().as_ref().deref().clone(),
            estimated_memory_usage: HashMap::from_iter([
                // todo: vfs memory usage
                // ("vfs".to_owned(), w.vfs.read().memory_usage()),
                // todo: analysis memory usage
                // ("analysis".to_owned(), cc.analysis.estimated_memory()),
            ]),
        };

        Ok(HashMap::from_iter([(dg, info)]))
    }
}

pub struct QuerySnap {
    rx: Arc<Mutex<Option<oneshot::Receiver<CompileSnapshot<LspCompilerFeat>>>>>,
    snap: tokio::sync::OnceCell<ZResult<CompileSnapshot<LspCompilerFeat>>>,
    handle: Arc<CompileHandler>,
}

impl QuerySnap {
    /// Snapshot the compiler thread for tasks
    pub async fn snapshot(&self) -> ZResult<CompileSnapshot<LspCompilerFeat>> {
        self.snap
            .get_or_init(|| async move {
                let rx = self.rx.lock().take().unwrap();
                rx.await.map_err(map_string_err("failed to get snapshot"))
            })
            .await
            .clone()
    }

    /// Snapshot the compiler thread for tasks
    pub fn snapshot_sync(&self) -> ZResult<CompileSnapshot<LspCompilerFeat>> {
        if let Some(snap) = self.snap.get() {
            return snap.clone();
        }

        let rx = self.rx.lock().take().unwrap();
        threaded_receive(rx).map_err(map_string_err("failed to get snapshot"))
    }

    pub fn stateful_sync<T: tinymist_query::StatefulRequest>(
        &self,
        req: T,
    ) -> anyhow::Result<Option<T::Response>> {
        let snap = self.snapshot_sync()?;
        let w = &snap.world;

        self.handle.run_analysis(w, |ctx| {
            req.request(
                ctx,
                snap.success_doc.map(|doc| VersionedDocument {
                    version: w.revision().get(),
                    document: doc,
                }),
            )
        })
    }

    pub async fn stateful<T: tinymist_query::StatefulRequest>(
        &self,
        req: T,
    ) -> anyhow::Result<Option<T::Response>> {
        let snap = self.snapshot().await?;
        let w = &snap.world;

        self.handle.run_analysis(w, |ctx| {
            req.request(
                ctx,
                snap.success_doc.map(|doc| VersionedDocument {
                    version: w.revision().get(),
                    document: doc,
                }),
            )
        })
    }

    pub fn semantic_sync<T: tinymist_query::SemanticRequest>(
        &self,
        req: T,
    ) -> anyhow::Result<Option<T::Response>> {
        let snap = self.snapshot_sync()?;
        let w = &snap.world;
        self.handle.run_analysis(w, |ctx| req.request(ctx))
    }

    pub async fn semantic<T: tinymist_query::SemanticRequest>(
        &self,
        req: T,
    ) -> anyhow::Result<Option<T::Response>> {
        let snap = self.snapshot().await?;
        let w = &snap.world;

        self.handle.run_analysis(w, |ctx| req.request(ctx))
    }
}
