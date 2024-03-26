//! The typst actors running compilations.

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
    diag::{SourceDiagnostic, SourceResult},
    util::Deferred,
};
#[cfg(feature = "preview")]
use typst_preview::{CompilationHandle, CompilationHandleImpl, CompileStatus};
use typst_ts_compiler::{
    service::{CompileDriverImpl, CompileEnv, CompileMiddleware, Compiler, EntryManager, EnvWorld},
    vfs::notify::{FileChangeSet, MemoryEvent},
};
use typst_ts_core::{
    config::compiler::EntryState, error::prelude::*, typst::prelude::EcoVec, Error, ImmutPath,
    TypstDocument, TypstWorld,
};

use super::compile::CompileClient as TsCompileClient;
use super::{compile::CompileActor as CompileActorInner, render::PdfExportConfig};
use crate::{actor::compile::EntryStateExt, ConstConfig};
use crate::{
    actor::render::{PdfPathVars, RenderActorRequest},
    utils,
};
use crate::{
    world::{LspWorld, LspWorldBuilder, SharedFontResolver},
    Config,
};

#[cfg(not(feature = "preview"))]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum CompileStatus {
    Compiling,
    CompileSuccess,
    CompileError,
}

#[cfg(not(feature = "preview"))]
pub trait CompilationHandle: Send + 'static {
    fn status(&self, status: CompileStatus);
    fn notify_compile(&self, res: Result<Arc<TypstDocument>, CompileStatus>);
}

type CompileDriverInner = CompileDriverImpl<LspWorld>;
type CompileService = CompileActorInner<CompileDriver>;
type CompileClient = TsCompileClient<CompileService>;

type DiagnosticsSender = mpsc::UnboundedSender<(String, Option<DiagnosticsMap>)>;

#[allow(clippy::too_many_arguments)]
pub fn create_server(
    diag_group: String,
    config: &Config,
    cfg: &ConstConfig,
    // opts: OptsState,
    font_resolver: Deferred<SharedFontResolver>,
    entry: EntryState,
    snapshot: FileChangeSet,
    diag_tx: DiagnosticsSender,
    doc_sender: watch::Sender<Option<Arc<TypstDocument>>>,
    render_tx: broadcast::Sender<RenderActorRequest>,
) -> CompileActor {
    let pos_encoding = cfg.position_encoding;

    let inner = Deferred::new({
        let current_runtime = tokio::runtime::Handle::current();
        let handler = CompileHandler {
            #[cfg(feature = "preview")]
            inner: Arc::new(Mutex::new(None)),
        };

        let diag_group = diag_group.clone();
        let entry = entry.clone();
        let render_tx = render_tx.clone();

        move || {
            info!("TypstActor: creating server for {diag_group}");

            let font_resolver = font_resolver.wait().clone();

            let world =
                LspWorldBuilder::build(entry.clone(), font_resolver).expect("incorrect options");
            let driver = CompileDriverInner::new(world);
            let driver = CompileDriver {
                inner: driver,
                handler,
                doc_sender,
                render_tx: render_tx.clone(),
                diag_group: diag_group.clone(),
                position_encoding: pos_encoding,
                diag_tx,
            };

            let actor = CompileActorInner::new(driver, entry).with_watch(true);
            let (server, client) = actor.split();

            // We do send memory changes instead of initializing compiler with them.
            // This is because there are state recorded inside of the compiler actor, and we
            // must update them.
            client.add_memory_changes(MemoryEvent::Update(snapshot));

            current_runtime.spawn(server.spawn());

            client
        }
    });

    CompileActor::new(
        diag_group,
        config.clone(),
        entry,
        pos_encoding,
        inner,
        render_tx,
    )
}

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

#[derive(Clone)]
pub struct CompileHandler {
    #[cfg(feature = "preview")]
    inner: Arc<Mutex<Option<CompilationHandleImpl>>>,
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

    fn notify_compile(&self, _result: Result<Arc<TypstDocument>, CompileStatus>) {
        #[cfg(feature = "preview")]
        {
            let inner = self.inner.lock();
            if let Some(inner) = inner.as_ref() {
                inner.notify_compile(_result.clone());
            }
        }
    }
}

pub struct CompileDriver {
    inner: CompileDriverInner,
    #[allow(unused)]
    handler: CompileHandler,

    doc_sender: watch::Sender<Option<Arc<TypstDocument>>>,
    render_tx: broadcast::Sender<RenderActorRequest>,

    diag_group: String,
    position_encoding: PositionEncoding,
    diag_tx: DiagnosticsSender,
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
        #[cfg(feature = "preview")]
        self.handler.status(CompileStatus::Compiling);
        match self.inner_mut().compile(env) {
            Ok(doc) => {
                #[cfg(feature = "preview")]
                self.handler.notify_compile(Ok(doc.clone()));

                let _ = self.doc_sender.send(Some(doc.clone()));
                // todo: is it right that ignore zero broadcast receiver?
                let _ = self.render_tx.send(RenderActorRequest::OnTyped);

                self.notify_diagnostics(EcoVec::new());
                Ok(doc)
            }
            Err(err) => {
                #[cfg(feature = "preview")]
                self.handler
                    .notify_compile(Err(CompileStatus::CompileError));

                self.notify_diagnostics(err);
                Err(EcoVec::new())
            }
        }
    }
}

impl CompileDriver {
    fn push_diagnostics(&mut self, diagnostics: Option<DiagnosticsMap>) {
        let err = self.diag_tx.send((self.diag_group.clone(), diagnostics));
        if let Err(err) = err {
            error!("failed to send diagnostics: {:#}", err);
        }
    }

    fn notify_diagnostics(&mut self, diagnostics: EcoVec<SourceDiagnostic>) {
        trace!("notify diagnostics: {:#?}", diagnostics);

        // todo encoding
        let w = self.inner.world_mut();
        // todo: root
        let root = w.entry.root().clone().unwrap();
        let diagnostics = tinymist_query::convert_diagnostics(
            &AnalysisContext::new(
                &WrapWorld(w),
                Analysis {
                    root,
                    position_encoding: self.position_encoding,
                },
            ),
            diagnostics.as_ref(),
            self.position_encoding,
        );

        // todo: better way to remove diagnostics
        // todo: check all errors in this file

        let detached = self.inner.world().entry.is_inactive();
        let valid = !detached;

        self.push_diagnostics(valid.then_some(diagnostics));
    }
}

pub struct CompileActor {
    diag_group: String,
    position_encoding: PositionEncoding,
    config: Config,
    // root_tx: Mutex<Option<oneshot::Sender<Option<ImmutPath>>>>,
    // root: OnceCell<Option<ImmutPath>>,
    entry: Arc<Mutex<EntryState>>,
    inner: Deferred<CompileClient>,
    render_tx: broadcast::Sender<RenderActorRequest>,
}

impl CompileActor {
    #[allow(clippy::too_many_arguments)]
    fn new(
        diag_group: String,
        config: Config,
        entry: EntryState,
        position_encoding: PositionEncoding,
        inner: Deferred<CompileClient>,
        render_tx: broadcast::Sender<RenderActorRequest>,
    ) -> Self {
        Self {
            diag_group,
            config,
            // root_tx: Mutex::new(root.is_none().then_some(root_tx)),
            // root: match root {
            //     Some(root) => OnceCell::from(Some(root)),
            //     None => OnceCell::new(),
            // },
            position_encoding,
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
                    compiler.compiler.compiler.push_diagnostics(None);
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

struct WrapWorld<'a>(&'a mut LspWorld);

impl<'a> AnaylsisResources for WrapWorld<'a> {
    fn world(&self) -> &dyn typst::World {
        self.0
    }

    fn resolve(
        &self,
        spec: &typst_ts_core::package::PackageSpec,
    ) -> Result<Arc<Path>, typst::diag::PackageError> {
        use typst_ts_compiler::package::Registry;
        self.0.registry.resolve(spec)
    }

    fn iter_dependencies(&self, f: &mut dyn FnMut(&ImmutPath, typst_ts_compiler::Time)) {
        use typst_ts_compiler::NotifyApi;
        self.0.iter_dependencies(f)
    }
}

impl CompileActor {
    pub fn query(&self, query: CompilerQueryRequest) -> anyhow::Result<CompilerQueryResponse> {
        use CompilerQueryRequest::*;
        assert!(query.fold_feature() != FoldRequestFeature::ContextFreeUnique);

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
        let enc = self.position_encoding;

        self.steal(move |compiler| {
            let doc = compiler.success_doc();
            let w = compiler.compiler.world_mut();

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

            let w = WrapWorld(w);
            Ok(f(
                &mut AnalysisContext::new(
                    &w,
                    Analysis {
                        root,
                        position_encoding: enc,
                    },
                ),
                doc,
            ))
        })?
    }

    fn steal_world<T: Send + Sync + 'static>(
        &self,
        f: impl FnOnce(&mut AnalysisContext) -> T + Send + Sync + 'static,
    ) -> anyhow::Result<T> {
        let enc = self.position_encoding;
        // let opts = match opts {
        //     OptsState::Exact(opts) => opts,
        //     OptsState::Rootless(opts) => {
        //         let root: ImmutPath = match utils::threaded_receive(root_rx) {
        //             Ok(Some(root)) => root,
        //             Ok(None) => {
        //                 error!("TypstActor: failed to receive root path: root is
        // none");                 return CompileClient::faked();
        //             }
        //             Err(err) => {
        //                 error!("TypstActor: failed to receive root path: {:#}", err);
        //                 return CompileClient::faked();
        //             }
        //         };

        //         opts(root.as_ref().into())
        //     }
        // };
        // mut opts: CompileOnceOpts,
        // let inputs = std::mem::take(&mut opts.inputs);
        // w.set_inputs(Arc::new(Prehashed::new(inputs)));

        self.steal(move |compiler| {
            let w = compiler.compiler.world_mut();

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

            let w = WrapWorld(w);
            Ok(f(&mut AnalysisContext::new(
                &w,
                Analysis {
                    root,
                    position_encoding: enc,
                },
            )))
        })?
    }
}

#[cfg(feature = "preview")]
mod preview_exts {
    use std::path::Path;

    use typst::layout::Position;
    use typst::syntax::Span;
    use typst_preview::{
        CompileHost, DocToSrcJumpInfo, EditorServer, Location, MemoryFiles, MemoryFilesShort,
        SourceFileServer,
    };
    use typst_ts_compiler::vfs::notify::FileChangeSet;
    use typst_ts_compiler::vfs::notify::MemoryEvent;
    use typst_ts_core::debug_loc::SourceSpanOffset;
    use typst_ts_core::Error;

    use super::CompileActor;

    #[cfg(feature = "preview")]
    impl SourceFileServer for CompileActor {
        async fn resolve_source_span(
            &mut self,
            loc: Location,
        ) -> Result<Option<SourceSpanOffset>, Error> {
            let Location::Src(src_loc) = loc;
            self.inner().resolve_src_location(src_loc).await
        }

        async fn resolve_document_position(
            &mut self,
            loc: Location,
        ) -> Result<Option<Position>, Error> {
            let Location::Src(src_loc) = loc;

            let path = Path::new(&src_loc.filepath).to_owned();
            let line = src_loc.pos.line;
            let column = src_loc.pos.column;

            self.inner()
                .resolve_src_to_doc_jump(path, line, column)
                .await
        }

        async fn resolve_source_location(
            &mut self,
            s: Span,
            offset: Option<usize>,
        ) -> Result<Option<DocToSrcJumpInfo>, Error> {
            Ok(self
                .inner()
                .resolve_span_and_offset(s, offset)
                .await
                .map_err(|err| {
                    log::error!("TypstActor: failed to resolve span and offset: {:#}", err);
                })
                .ok()
                .flatten()
                .map(|e| DocToSrcJumpInfo {
                    filepath: e.filepath,
                    start: e.start,
                    end: e.end,
                }))
        }
    }

    #[cfg(feature = "preview")]
    impl EditorServer for CompileActor {
        async fn update_memory_files(
            &mut self,
            files: MemoryFiles,
            reset_shadow: bool,
        ) -> Result<(), Error> {
            // todo: is it safe to believe that the path is normalized?
            let now = std::time::SystemTime::now();
            let files = FileChangeSet::new_inserts(
                files
                    .files
                    .into_iter()
                    .map(|(path, content)| {
                        let content = content.as_bytes().into();
                        // todo: cloning PathBuf -> Arc<Path>
                        (path.into(), Ok((now, content)).into())
                    })
                    .collect(),
            );
            self.inner().add_memory_changes(if reset_shadow {
                MemoryEvent::Sync(files)
            } else {
                MemoryEvent::Update(files)
            });

            Ok(())
        }

        async fn remove_shadow_files(&mut self, files: MemoryFilesShort) -> Result<(), Error> {
            // todo: is it safe to believe that the path is normalized?
            let files =
                FileChangeSet::new_removes(files.files.into_iter().map(From::from).collect());
            self.inner().add_memory_changes(MemoryEvent::Update(files));

            Ok(())
        }
    }

    #[cfg(feature = "preview")]
    impl CompileHost for CompileActor {}
}
