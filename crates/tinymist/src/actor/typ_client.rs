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
    diag::{PackageError, SourceDiagnostic, SourceResult},
    layout::Position,
    model::Document as TypstDocument,
    syntax::package::PackageSpec,
    World as TypstWorld,
};
use typst_ts_compiler::{
    vfs::notify::MemoryEvent, CompileEnv, CompileMiddleware, Compiler, EntryReader, PureCompiler,
    TaskInputs,
};
use typst_ts_core::{
    config::compiler::EntryState, debug_loc::DataSource, error::prelude::*, typst::prelude::EcoVec,
    Error, ImmutPath, TypstFont,
};

use super::{
    editor::{EditorRequest, TinymistCompileStatusEnum},
    export::ExportConfig,
    typ_server::{CompileSnapshot, Interrupt},
};
use crate::{
    actor::export::ExportRequest,
    compile_init::CompileConfig,
    tools::preview::{CompilationHandle, CompileStatus},
    utils::{self, threaded_receive},
    world::{LspCompilerFeat, LspWorld},
};

pub type CompileClientActor = CompileClientActorImpl;

type EditorSender = mpsc::UnboundedSender<EditorRequest>;

pub struct CompileHandler {
    pub(super) diag_group: String,

    #[cfg(feature = "preview")]
    pub(super) inner: Arc<Mutex<Option<typst_preview::CompilationHandleImpl>>>,

    pub(super) doc_tx: watch::Sender<Option<Arc<TypstDocument>>>,
    pub(super) export_tx: mpsc::UnboundedSender<ExportRequest>,
    pub(super) editor_tx: EditorSender,
}

impl CompilationHandle for CompileHandler {
    fn status(&self, _status: CompileStatus) {
        #[cfg(feature = "preview")]
        if let Some(inner) = self.inner.lock().as_ref() {
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
        if let Some(inner) = self.inner.lock().as_ref() {
            inner.notify_compile(res);
        }
    }
}

impl CompileHandler {
    fn push_diagnostics(&mut self, diagnostics: Option<DiagnosticsMap>) {
        let res = self
            .editor_tx
            .send(EditorRequest::Diag(self.diag_group.clone(), diagnostics));
        if let Err(err) = res {
            error!("failed to send diagnostics: {err:#}");
        }
    }
}

pub struct CompileDriver {
    pub(super) inner: PureCompiler<LspWorld>,
    #[allow(unused)]
    pub(super) handler: CompileHandler,
    pub(super) analysis: Analysis,
    pub(super) periscope: PeriscopeRenderer,
}

impl CompileMiddleware for CompileDriver {
    type Compiler = PureCompiler<LspWorld>;

    fn inner(&self) -> &Self::Compiler {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Self::Compiler {
        &mut self.inner
    }

    fn wrap_compile(
        &mut self,
        world: &LspWorld,
        env: &mut CompileEnv,
    ) -> SourceResult<Arc<typst::model::Document>> {
        self.handler
            .editor_tx
            .send(EditorRequest::Status(
                self.handler.diag_group.clone(),
                TinymistCompileStatusEnum::Compiling,
            ))
            .unwrap();
        self.handler.status(CompileStatus::Compiling);
        match self
            .ensure_main(world)
            .and_then(|_| self.inner_mut().compile(world, env))
        {
            Ok(doc) => {
                self.handler.notify_compile(Ok(doc.clone()));
                self.notify_diagnostics(
                    world,
                    EcoVec::new(),
                    env.tracer.as_ref().map(|e| e.clone().warnings()),
                );
                Ok(doc)
            }
            Err(err) => {
                self.handler
                    .notify_compile(Err(CompileStatus::CompileError));
                self.notify_diagnostics(
                    world,
                    err,
                    env.tracer.as_ref().map(|e| e.clone().warnings()),
                );
                Err(EcoVec::new())
            }
        }
    }
}

impl CompileDriver {
    fn notify_diagnostics(
        &mut self,
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
                self.handler.push_diagnostics(valid.then_some(diagnostics));
            }
            Err(err) => {
                error!("TypstActor: failed to convert diagnostics: {:#}", err);
                self.handler.push_diagnostics(None);
            }
        }
    }

    pub fn run_analysis<T>(
        &mut self,
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

        self.analysis.root = root;
        Ok(f(&mut AnalysisContext::new_borrow(&w, &mut self.analysis)))
    }
}

pub struct CompileClientActorImpl {
    pub diag_group: String,
    pub config: CompileConfig,
    entry: EntryState,
    intr_tx: mpsc::UnboundedSender<Interrupt<LspCompilerFeat>>,
    export_tx: mpsc::UnboundedSender<ExportRequest>,
}

impl CompileClientActorImpl {
    pub(crate) fn new(
        diag_group: String,
        config: CompileConfig,
        entry: EntryState,
        intr_tx: mpsc::UnboundedSender<Interrupt<LspCompilerFeat>>,
        export_tx: mpsc::UnboundedSender<ExportRequest>,
    ) -> Self {
        Self {
            diag_group,
            config,
            entry,
            intr_tx,
            export_tx,
        }
    }

    /// Snapshot the compiler thread for tasks
    pub async fn snapshot(&self) -> ZResult<CompileSnapshot<LspCompilerFeat>> {
        let (tx, rx) = oneshot::channel();

        self.intr_tx
            .send(Interrupt::Snapshot(tx))
            .map_err(map_string_err("failed to send snapshot request"))?;
        rx.await.map_err(map_string_err("failed to get snapshot"))
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
        let _ = self.export_tx.send(ExportRequest::ChangeConfig(config));
    }

    pub fn on_export(&self, kind: ExportKind, path: PathBuf) -> anyhow::Result<Option<PathBuf>> {
        // todo: we currently doesn't respect the path argument...
        info!("CompileActor: on export: {}", path.display());

        let (tx, rx) = oneshot::channel();
        let _ = self.export_tx.send(ExportRequest::Oneshot(Some(kind), tx));
        let res: Option<PathBuf> = utils::threaded_receive(rx)?;

        info!("CompileActor: on export end: {path:?} as {res:?}");
        Ok(res)
    }

    pub fn on_save_export(&self, path: PathBuf) -> anyhow::Result<()> {
        info!("CompileActor: on save export: {}", path.display());
        let _ = self.export_tx.send(ExportRequest::OnSaved);

        Ok(())
    }
}

impl CompileClientActorImpl {
    pub fn run_analysis<T>(
        &self,
        w: &LspWorld,
        f: impl FnOnce(&mut AnalysisContext<'_>) -> T,
    ) -> anyhow::Result<T> {
        let _ = w;
        let _ = f;
        todo!()
    }

    pub fn settle(&mut self) {
        let _ = self.change_entry(None);
        info!("TypstActor({}): settle requested", self.diag_group);
        let (tx, rx) = oneshot::channel();
        let _ = self.intr_tx.send(Interrupt::Settle(tx));
        match utils::threaded_receive(rx) {
            Ok(()) => info!("TypstActor({}): settled", self.diag_group),
            Err(err) => error!("TypstActor({}): failed to settle: {err:#}", self.diag_group),
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

        let diag_group = &self.diag_group;
        info!("the entry file of TypstActor({diag_group}) is changing to {next_entry:?}");

        let next = next_entry.clone();
        // todo: remove diags at typ_server.rs
        // let next_is_inactive = next.is_inactive();

        self.change_task(TaskInputs {
            entry: Some(next.clone()),
            ..Default::default()
        });

        let next = next_entry.clone();
        let _ = self.export_tx.send(ExportRequest::ChangeExportPath(next));

        self.entry = next_entry;

        Ok(true)
    }

    pub fn steal_state<T: Send + Sync + 'static>(
        &self,
        f: impl FnOnce(&mut AnalysisContext, Option<VersionedDocument>) -> T + Send + Sync + 'static,
    ) -> anyhow::Result<T> {
        let snap = self.sync_snapshot()?;
        self.run_analysis(&snap.world, |ctx| {
            f(
                ctx,
                snap.success_doc.map(|doc| VersionedDocument {
                    version: snap.world.revision().get(),
                    document: doc,
                }),
            )
        })
    }

    pub fn steal_world<T: Send + Sync + 'static>(
        &self,
        f: impl FnOnce(&mut AnalysisContext) -> T + Send + Sync + 'static,
    ) -> anyhow::Result<T> {
        let snap = self.sync_snapshot()?;
        self.run_analysis(&snap.world, f)
    }

    pub fn clear_cache(&self) {
        // let _ = self.steal(|c| {
        //     c.compiler.compiler.analysis.caches = Default::default();
        // });
        todo!()
    }

    pub fn collect_server_info(&self) -> anyhow::Result<HashMap<String, ServerInfoResponse>> {
        let dg = self.diag_group.clone();

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
