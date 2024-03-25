//! The typst actors running compilations.
//!
//! ```ascii
//! ┌────────────────────────────────┐                      
//! │    main::compile_actor (client)│                      
//! └─────┬────────────────────▲─────┘                      
//!       │                    │                            
//! ┌─────▼────────────────────┴─────┐         ┌────────────┐
//! │compiler::compile_actor (server)│◄───────►│notify_actor│
//! └─────┬────────────────────▲─────┘         └────────────┘
//!       │                    │                            
//! ┌─────▼────────────────────┴─────┐ handler ┌────────────┐
//! │compiler::compile_driver        ├────────►│ rest actors│
//! └────────────────────────────────┘         └────────────┘
//! ```
//!
//! We generally use typst in two ways.
//! + creates a [`CompileDriver`] and run compilation in fly.
//! + creates a [`CompileServerActor`], wraps the drvier, and runs
//!   [`CompileDriver`] incrementally.
//!
//! For latter case, an additional [`CompileClientActor`] is created to
//! control the [`CompileServerActor`].
//!
//! The [`CompileDriver`] will also keep a [`CompileHandler`] to push
//! information to other actors.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::anyhow;
use log::{error, info, trace};
use parking_lot::Mutex;
use tinymist_query::{
    analysis::{Analysis, AnalysisContext, AnaylsisResources},
    CompilerQueryRequest, CompilerQueryResponse, DiagnosticsMap, FoldRequestFeature,
    OnExportRequest, OnSaveExportRequest, PositionEncoding, SemanticRequest, StatefulRequest,
    VersionedDocument,
};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use typst::{
    diag::{PackageError, SourceDiagnostic, SourceResult},
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
    config::compiler::EntryState, error::prelude::*, typst::prelude::EcoVec, Error, ImmutPath,
};

use super::typ_server::CompileClient as TsCompileClient;
use super::{render::PdfExportConfig, typ_server::CompileServerActor};
use crate::{
    actor::render::{PdfPathVars, RenderActorRequest},
    utils,
};
use crate::{
    actor::typ_server::EntryStateExt,
    tools::preview::{CompilationHandle, CompileStatus},
};
use crate::{world::LspWorld, Config};

type CompileDriverInner = CompileDriverImpl<LspWorld>;
type CompileService = CompileServerActor<CompileDriver>;
type CompileClient = TsCompileClient<CompileService>;

type DiagnosticsSender = mpsc::UnboundedSender<(String, Option<DiagnosticsMap>)>;

pub struct CompileHandler {
    pub(super) diag_group: String,

    #[cfg(feature = "preview")]
    pub(super) inner: Arc<Mutex<Option<typst_preview::CompilationHandleImpl>>>,

    pub(super) doc_tx: watch::Sender<Option<Arc<TypstDocument>>>,
    pub(super) render_tx: broadcast::Sender<RenderActorRequest>,
    pub(super) diag_tx: DiagnosticsSender,
}

impl CompilationHandle for CompileHandler {
    fn status(&self, _status: CompileStatus) {
        #[cfg(feature = "preview")]
        {
            let inner = self.inner.lock();
            if let Some(inner) = inner.as_ref() {
                inner.status(_status);
            }
        }
    }

    fn notify_compile(&self, res: Result<Arc<TypstDocument>, CompileStatus>) {
        if let Ok(doc) = res.clone() {
            let _ = self.doc_tx.send(Some(doc.clone()));
            // todo: is it right that ignore zero broadcast receiver?
            let _ = self.render_tx.send(RenderActorRequest::OnTyped);
        }

        #[cfg(feature = "preview")]
        {
            let inner = self.inner.lock();
            if let Some(inner) = inner.as_ref() {
                inner.notify_compile(res);
            }
        }
    }
}

impl CompileHandler {
    fn push_diagnostics(&mut self, diagnostics: Option<DiagnosticsMap>) {
        let err = self.diag_tx.send((self.diag_group.clone(), diagnostics));
        if let Err(err) = err {
            error!("failed to send diagnostics: {:#}", err);
        }
    }
}

pub struct CompileDriver {
    pub(super) inner: CompileDriverInner,
    #[allow(unused)]
    pub(super) handler: CompileHandler,
    pub(super) position_encoding: PositionEncoding,
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
        self.handler.status(CompileStatus::Compiling);
        match self.inner_mut().compile(env) {
            Ok(doc) => {
                self.handler.notify_compile(Ok(doc.clone()));
                self.notify_diagnostics(EcoVec::new());
                Ok(doc)
            }
            Err(err) => {
                self.handler
                    .notify_compile(Err(CompileStatus::CompileError));
                self.notify_diagnostics(err);
                Err(EcoVec::new())
            }
        }
    }
}

impl CompileDriver {
    fn notify_diagnostics(&mut self, diagnostics: EcoVec<SourceDiagnostic>) {
        trace!("notify diagnostics: {:#?}", diagnostics);

        let diagnostics =
            self.run_analysis(|ctx| tinymist_query::convert_diagnostics(ctx, diagnostics.as_ref()));

        match diagnostics {
            Ok(diagnostics) => {
                // todo: better way to remove diagnostics
                // todo: check all errors in this file
                let detached = self.inner.world().entry.is_inactive();
                let valid = !detached;
                self.handler.push_diagnostics(valid.then_some(diagnostics));
            }
            Err(err) => {
                log::error!("TypstActor: failed to convert diagnostics: {:#}", err);
                self.handler.push_diagnostics(None);
            }
        }
    }

    fn run_analysis<T>(
        &mut self,
        f: impl FnOnce(&mut AnalysisContext<'_>) -> T,
    ) -> anyhow::Result<T> {
        let enc = self.position_encoding;
        let w = self.inner.world_mut();

        let Some(main) = w.main_id() else {
            log::error!("TypstActor: main file is not set");
            return Err(anyhow!("main file is not set"));
        };
        let Some(root) = w.entry.root() else {
            log::error!("TypstActor: root is not set");
            return Err(anyhow!("root is not set"));
        };
        w.source(main).map_err(|err| {
            log::info!("TypstActor: failed to prepare main file: {:?}", err);
            anyhow!("failed to get source: {err}")
        })?;
        w.prepare_env(&mut Default::default()).map_err(|err| {
            log::error!("TypstActor: failed to prepare env: {:?}", err);
            anyhow!("failed to prepare env")
        })?;

        struct WrapWorld<'a>(&'a mut LspWorld);

        impl<'a> AnaylsisResources for WrapWorld<'a> {
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
        }

        let w = WrapWorld(w);
        Ok(f(&mut AnalysisContext::new(
            &w,
            Analysis {
                root,
                position_encoding: enc,
            },
        )))
    }
}

pub struct CompileClientActor {
    diag_group: String,
    config: Config,
    entry: Arc<Mutex<EntryState>>,
    inner: Deferred<CompileClient>,
    render_tx: broadcast::Sender<RenderActorRequest>,
}

impl CompileClientActor {
    pub(crate) fn new(
        diag_group: String,
        config: Config,
        entry: EntryState,
        inner: Deferred<CompileClient>,
        render_tx: broadcast::Sender<RenderActorRequest>,
    ) -> Self {
        Self {
            diag_group,
            config,
            entry: Arc::new(Mutex::new(entry)),
            inner,
            render_tx,
        }
    }

    fn inner(&self) -> &CompileClient {
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
        f: impl FnOnce(&mut CompileService, tokio::runtime::Handle) -> Ret + Send + 'static,
    ) -> ZResult<Ret> {
        self.inner().steal_async(f).await
    }

    pub fn settle(&self) {
        let _ = self.change_entry(None);
        info!("TypstActor({}): settle requested", self.diag_group);
        let res = self.inner().settle();
        match res {
            Ok(()) => info!("TypstActor({}): settled", self.diag_group),
            Err(err) => {
                error!(
                    "TypstActor({}): failed to settle: {:#}",
                    self.diag_group, err
                );
            }
        }
    }

    pub fn change_entry(&self, path: Option<ImmutPath>) -> Result<(), Error> {
        if path.as_deref().is_some_and(|p| !p.is_absolute()) {
            return Err(error_once!("entry file must be absolute", path: path.unwrap().display()));
        }

        let next_entry = self.config.determine_entry(path);

        // todo: more robust rollback logic
        let entry = self.entry.clone();
        let should_change = {
            let prev_entry = entry.lock();
            let should_change = next_entry != *prev_entry;
            should_change.then(|| prev_entry.clone())
        };

        if let Some(prev) = should_change {
            let next = next_entry.clone();

            info!(
                "the entry file of TypstActor({}) is changing to {next:?}",
                self.diag_group,
            );

            self.render_tx
                .send(RenderActorRequest::ChangeExportPath(PdfPathVars {
                    entry: next.clone(),
                }))
                .unwrap();

            // todo
            let res = self.steal(move |compiler| {
                compiler.change_entry(next.clone());

                let next_is_inactive = next.is_inactive();
                let res = compiler.compiler.world_mut().mutate_entry(next);

                if next_is_inactive {
                    info!("TypstActor: removing diag");
                    compiler.compiler.compiler.handler.push_diagnostics(None);
                }

                res.map(|_| ())
                    .map_err(|err| error_once!("failed to change entry", err: format!("{err:?}")))
            });

            let res = match res {
                Ok(res) => res,
                Err(res) => Err(res),
            };

            if res.is_err() {
                self.render_tx
                    .send(RenderActorRequest::ChangeExportPath(PdfPathVars {
                        entry: prev.clone(),
                    }))
                    .unwrap();

                let mut entry = entry.lock();
                // todo: the rollback is actually not atomic
                if *entry == next_entry {
                    *entry = prev;
                }

                return res;
            }

            // todo: trigger recompile
            let files = FileChangeSet::new_inserts(vec![]);
            self.inner().add_memory_changes(MemoryEvent::Update(files));
        }

        Ok(())
    }

    pub fn add_memory_changes(&self, event: MemoryEvent) {
        self.inner.wait().add_memory_changes(event);
    }

    pub(crate) fn change_export_pdf(&self, config: PdfExportConfig) {
        let entry = self.entry.lock().clone();
        let _ = self
            .render_tx
            .send(RenderActorRequest::ChangeConfig(PdfExportConfig {
                substitute_pattern: config.substitute_pattern,
                // root: self.root.get().cloned().flatten(),
                entry,
                mode: config.mode,
            }))
            .unwrap();
    }
}

impl CompileClientActor {
    pub fn query(&self, query: CompilerQueryRequest) -> anyhow::Result<CompilerQueryResponse> {
        use CompilerQueryRequest::*;
        assert!(query.fold_feature() != FoldRequestFeature::ContextFreeUnique);

        macro_rules! query_state {
            ($self:ident, $method:ident, $req:expr) => {{
                let res = $self.steal_state(move |w, doc| $req.request(w, doc));
                res.map(CompilerQueryResponse::$method)
            }};
        }

        macro_rules! query_world {
            ($self:ident, $method:ident, $req:expr) => {{
                let res = $self.steal_world(move |w| $req.request(w));
                res.map(CompilerQueryResponse::$method)
            }};
        }

        match query {
            CompilerQueryRequest::OnExport(OnExportRequest { path }) => {
                Ok(CompilerQueryResponse::OnExport(self.on_export(path)?))
            }
            CompilerQueryRequest::OnSaveExport(OnSaveExportRequest { path }) => {
                self.on_save_export(path)?;
                Ok(CompilerQueryResponse::OnSaveExport(()))
            }
            Hover(req) => query_state!(self, Hover, req),
            GotoDefinition(req) => query_world!(self, GotoDefinition, req),
            GotoDeclaration(req) => query_world!(self, GotoDeclaration, req),
            References(req) => query_world!(self, References, req),
            InlayHint(req) => query_world!(self, InlayHint, req),
            CodeLens(req) => query_world!(self, CodeLens, req),
            Completion(req) => query_state!(self, Completion, req),
            SignatureHelp(req) => query_world!(self, SignatureHelp, req),
            Rename(req) => query_world!(self, Rename, req),
            PrepareRename(req) => query_world!(self, PrepareRename, req),
            Symbol(req) => query_world!(self, Symbol, req),
            FoldingRange(..)
            | SelectionRange(..)
            | SemanticTokensDelta(..)
            | DocumentSymbol(..)
            | SemanticTokensFull(..) => unreachable!(),
        }
    }

    fn on_export(&self, path: PathBuf) -> anyhow::Result<Option<PathBuf>> {
        info!("CompileActor: on export: {}", path.display());

        let (tx, rx) = oneshot::channel();

        let task = Arc::new(Mutex::new(Some(tx)));

        self.render_tx
            .send(RenderActorRequest::DoExport(task))
            .map_err(map_string_err("failed to send to sync_render"))?;

        let res: Option<PathBuf> = utils::threaded_receive(rx)?;

        info!("CompileActor: on export end: {path:?} as {res:?}");

        Ok(res)
    }

    fn on_save_export(&self, path: PathBuf) -> anyhow::Result<()> {
        info!("CompileActor: on save export: {}", path.display());
        let _ = self.render_tx.send(RenderActorRequest::OnSaved(path));

        Ok(())
    }

    fn steal_state<T: Send + Sync + 'static>(
        &self,
        f: impl FnOnce(&mut AnalysisContext, Option<VersionedDocument>) -> T + Send + Sync + 'static,
    ) -> anyhow::Result<T> {
        self.steal(move |compiler| {
            let doc = compiler.success_doc();
            let c = &mut compiler.compiler.compiler;
            c.run_analysis(move |ctx| f(ctx, doc))
        })?
    }

    fn steal_world<T: Send + Sync + 'static>(
        &self,
        f: impl FnOnce(&mut AnalysisContext) -> T + Send + Sync + 'static,
    ) -> anyhow::Result<T> {
        self.steal(move |compiler| compiler.compiler.compiler.run_analysis(f))?
    }
}
