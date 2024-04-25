//! The typst actors running compilations.
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

use anyhow::anyhow;
use log::{error, info, trace};
use parking_lot::Mutex;
use tinymist_query::{
    analysis::{Analysis, AnalysisContext, AnalysisResources},
    DiagnosticsMap, ExportKind, ServerInfoResponse, VersionedDocument,
};
use tinymist_render::PeriscopeRenderer;
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use typst::{
    diag::{PackageError, SourceDiagnostic, SourceResult},
    layout::Position,
    model::Document as TypstDocument,
    syntax::package::PackageSpec,
    util::Deferred,
    World as TypstWorld,
};
use typst_ts_compiler::{
    service::{CompileDriverImpl, CompileEnv, CompileMiddleware, Compiler, EntryManager, EnvWorld},
    vfs::notify::{FileChangeSet, MemoryEvent},
    Time,
};
use typst_ts_core::{
    config::compiler::EntryState, debug_loc::DataSource, error::prelude::*, typst::prelude::EcoVec,
    Error, ImmutPath, TypstFont,
};

use super::{
    cluster::{CompileClusterRequest, TinymistCompileStatusEnum},
    render::ExportConfig,
    typ_server::{CompileClient as TsCompileClient, CompileServerActor},
};
use crate::{
    actor::render::{OneshotRendering, PathVars, RenderActorRequest},
    actor::typ_server::EntryStateExt,
    compiler_init::CompileConfig,
    tools::preview::{CompilationHandle, CompileStatus},
    utils,
    world::LspWorld,
};

type CompileDriverInner = CompileDriverImpl<LspWorld>;
type CompileService = CompileServerActor<CompileDriver>;
type CompileClient = TsCompileClient<CompileService>;

type EditorSender = mpsc::UnboundedSender<CompileClusterRequest>;

pub struct CompileHandler {
    pub(super) diag_group: String,

    #[cfg(feature = "preview")]
    pub(super) inner: Arc<Mutex<Option<typst_preview::CompilationHandleImpl>>>,

    pub(super) doc_tx: watch::Sender<Option<Arc<TypstDocument>>>,
    pub(super) render_tx: broadcast::Sender<RenderActorRequest>,
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
            // todo: is it right that ignore zero broadcast receiver?
            let _ = self.render_tx.send(RenderActorRequest::OnTyped);
        }

        self.editor_tx
            .send(CompileClusterRequest::Status(
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
        let res = self.editor_tx.send(CompileClusterRequest::Diag(
            self.diag_group.clone(),
            diagnostics,
        ));
        if let Err(err) = res {
            error!("failed to send diagnostics: {err:#}");
        }
    }
}

pub struct CompileDriver {
    pub(super) inner: CompileDriverInner,
    #[allow(unused)]
    pub(super) handler: CompileHandler,
    pub(super) analysis: Analysis,
    pub(super) periscope: PeriscopeRenderer,
}

impl CompileMiddleware for CompileDriver {
    type Compiler = CompileDriverInner;

    fn inner(&self) -> &Self::Compiler {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Self::Compiler {
        &mut self.inner
    }

    fn wrap_compile(&mut self, env: &mut CompileEnv) -> SourceResult<Arc<typst::model::Document>> {
        self.handler
            .editor_tx
            .send(CompileClusterRequest::Status(
                self.handler.diag_group.clone(),
                TinymistCompileStatusEnum::Compiling,
            ))
            .unwrap();
        self.handler.status(CompileStatus::Compiling);
        match self.inner_mut().compile(env) {
            Ok(doc) => {
                self.handler.notify_compile(Ok(doc.clone()));
                self.notify_diagnostics(
                    EcoVec::new(),
                    env.tracer.as_ref().map(|e| e.clone().warnings()),
                );
                Ok(doc)
            }
            Err(err) => {
                self.handler
                    .notify_compile(Err(CompileStatus::CompileError));
                self.notify_diagnostics(err, env.tracer.as_ref().map(|e| e.clone().warnings()));
                Err(EcoVec::new())
            }
        }
    }
}

impl CompileDriver {
    fn notify_diagnostics(
        &mut self,
        errors: EcoVec<SourceDiagnostic>,
        warnings: Option<EcoVec<SourceDiagnostic>>,
    ) {
        trace!("notify diagnostics: {errors:#?} {warnings:#?}");

        let diagnostics = self.run_analysis(|ctx| {
            tinymist_query::convert_diagnostics(ctx, errors.iter().chain(warnings.iter().flatten()))
        });

        match diagnostics {
            Ok(diagnostics) => {
                // todo: better way to remove diagnostics
                // todo: check all errors in this file
                let detached = self.inner.world().entry.is_inactive();
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
        f: impl FnOnce(&mut AnalysisContext<'_>) -> T,
    ) -> anyhow::Result<T> {
        let w = self.inner.world_mut();

        let Some(main) = w.main_id() else {
            error!("TypstActor: main file is not set");
            return Err(anyhow!("main file is not set"));
        };
        let Some(root) = w.entry.root() else {
            error!("TypstActor: root is not set");
            return Err(anyhow!("root is not set"));
        };
        w.source(main).map_err(|err| {
            info!("TypstActor: failed to prepare main file: {err:?}");
            anyhow!("failed to get source: {err}")
        })?;
        w.prepare_env(&mut Default::default()).map_err(|err| {
            error!("TypstActor: failed to prepare env: {err:?}");
            anyhow!("failed to prepare env")
        })?;

        struct WrapWorld<'a>(&'a mut LspWorld, &'a PeriscopeRenderer);

        impl<'a> AnalysisResources for WrapWorld<'a> {
            fn world(&self) -> &dyn typst::World {
                self.0
            }

            fn resolve(&self, spec: &PackageSpec) -> Result<Arc<Path>, PackageError> {
                use typst_ts_compiler::package::Registry;
                self.0.registry.resolve(spec)
            }

            fn iter_dependencies(&self, f: &mut dyn FnMut(&ImmutPath, Time)) {
                use typst_ts_compiler::NotifyApi;
                self.0.iter_dependencies(f)
            }

            /// Resolve extra font information.
            fn font_info(&self, font: TypstFont) -> Option<Arc<DataSource>> {
                self.0.font_resolver.inner.describe_font(&font)
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

pub struct CompileClientActor {
    pub diag_group: String,
    pub config: CompileConfig,
    entry: EntryState,
    inner: Deferred<CompileClient>,
    render_tx: broadcast::Sender<RenderActorRequest>,
}

impl CompileClientActor {
    pub(crate) fn new(
        diag_group: String,
        config: CompileConfig,
        entry: EntryState,
        inner: Deferred<CompileClient>,
        render_tx: broadcast::Sender<RenderActorRequest>,
    ) -> Self {
        Self {
            diag_group,
            config,
            entry,
            inner,
            render_tx,
        }
    }

    pub fn inner(&self) -> &CompileClient {
        self.inner.wait()
    }

    /// Steal the compiler thread and run the given function.
    pub fn steal<Ret: Send + 'static>(
        &self,
        f: impl FnOnce(&mut CompileService) -> Ret + Send + 'static,
    ) -> ZResult<Ret> {
        self.inner().steal(f)
    }

    /// Steal the compiler thread and run the given function.
    pub async fn steal_async<Ret: Send + 'static>(
        &self,
        f: impl FnOnce(&mut CompileService) -> Ret + Send + 'static,
    ) -> ZResult<Ret> {
        self.inner().steal_async(f).await
    }

    pub fn settle(&mut self) {
        let _ = self.change_entry(None);
        info!("TypstActor({}): settle requested", self.diag_group);
        match self.inner().settle() {
            Ok(()) => info!("TypstActor({}): settled", self.diag_group),
            Err(err) => error!("TypstActor({}): failed to settle: {err:#}", self.diag_group),
        }
    }

    pub fn sync_config(&mut self, config: CompileConfig) {
        self.config = config;
    }

    pub fn change_entry(&mut self, path: Option<ImmutPath>) -> Result<(), Error> {
        if path
            .as_deref()
            .is_some_and(|p| !p.is_absolute() && !p.starts_with("/untitled"))
        {
            return Err(error_once!("entry file must be absolute", path: path.unwrap().display()));
        }

        let next_entry = self.config.determine_entry(path);
        if next_entry == self.entry {
            return Ok(());
        }

        let diag_group = &self.diag_group;
        info!("the entry file of TypstActor({diag_group}) is changing to {next_entry:?}");

        // todo
        let next = next_entry.clone();
        self.steal(move |compiler| {
            compiler.change_entry(next.clone());

            let next_is_inactive = next.is_inactive();
            let res = compiler.compiler.world_mut().mutate_entry(next);

            if next_is_inactive {
                info!("TypstActor: removing diag");
                compiler.compiler.compiler.handler.push_diagnostics(None);
            }

            res.map(|_| ())
                .map_err(|err| error_once!("failed to change entry", err: format!("{err:?}")))
        })??;

        let entry = next_entry.clone();
        let req = RenderActorRequest::ChangeExportPath(PathVars { entry });
        self.render_tx.send(req).unwrap();

        // todo: better way to trigger recompile
        let files = FileChangeSet::new_inserts(vec![]);
        self.inner().add_memory_changes(MemoryEvent::Update(files));

        self.entry = next_entry;

        Ok(())
    }

    pub fn add_memory_changes(&self, event: MemoryEvent) {
        self.inner.wait().add_memory_changes(event);
    }

    pub(crate) fn change_export_pdf(&mut self, config: ExportConfig) {
        let _ = self
            .render_tx
            .send(RenderActorRequest::ChangeConfig(ExportConfig {
                substitute_pattern: config.substitute_pattern,
                entry: self.entry.clone(),
                mode: config.mode,
            }))
            .unwrap();
    }

    pub fn clear_cache(&self) {
        let _ = self.steal(|c| {
            c.compiler.compiler.analysis.caches = Default::default();
        });
    }

    pub fn collect_server_info(&self) -> anyhow::Result<HashMap<String, ServerInfoResponse>> {
        let dg = self.diag_group.clone();
        self.steal(move |c| {
            let cc = &c.compiler.compiler;

            let info = ServerInfoResponse {
                root: cc.world().entry.root().map(|e| e.as_ref().to_owned()),
                font_paths: cc.world().font_resolver.font_paths().to_owned(),
                inputs: cc.world().inputs.as_ref().deref().clone(),
                estimated_memory_usage: HashMap::from_iter([
                    ("vfs".to_owned(), cc.world().vfs.memory_usage()),
                    ("analysis".to_owned(), cc.analysis.estimated_memory()),
                ]),
            };

            HashMap::from_iter([(dg, info)])
        })
        .map_err(|e| e.into())
    }

    pub fn on_export(&self, kind: ExportKind, path: PathBuf) -> anyhow::Result<Option<PathBuf>> {
        // todo: we currently doesn't respect the path argument...
        info!("CompileActor: on export: {}", path.display());

        let (tx, rx) = oneshot::channel();

        let callback = Arc::new(Mutex::new(Some(tx)));
        self.render_tx
            .send(RenderActorRequest::Oneshot(OneshotRendering {
                kind: Some(kind),
                callback,
            }))
            .map_err(map_string_err("failed to send to sync_render"))?;

        let res: Option<PathBuf> = utils::threaded_receive(rx)?;

        info!("CompileActor: on export end: {path:?} as {res:?}");

        Ok(res)
    }

    pub fn on_save_export(&self, path: PathBuf) -> anyhow::Result<()> {
        info!("CompileActor: on save export: {}", path.display());
        let _ = self.render_tx.send(RenderActorRequest::OnSaved(path));

        Ok(())
    }

    pub fn steal_state<T: Send + Sync + 'static>(
        &self,
        f: impl FnOnce(&mut AnalysisContext, Option<VersionedDocument>) -> T + Send + Sync + 'static,
    ) -> anyhow::Result<T> {
        self.steal(move |compiler| {
            let doc = compiler.success_doc();
            let c = &mut compiler.compiler.compiler;
            c.run_analysis(move |ctx| f(ctx, doc))
        })?
    }

    pub fn steal_world<T: Send + Sync + 'static>(
        &self,
        f: impl FnOnce(&mut AnalysisContext) -> T + Send + Sync + 'static,
    ) -> anyhow::Result<T> {
        self.steal(move |compiler| compiler.compiler.compiler.run_analysis(f))?
    }
}
