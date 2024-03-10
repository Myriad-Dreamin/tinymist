//! The typst actors running compilations.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex as SyncMutex},
};

use log::{debug, error, trace, warn};
use parking_lot::Mutex;
use tinymist_query::{
    CompilerQueryRequest, CompilerQueryResponse, DiagnosticsMap, FoldRequestFeature,
    OnSaveExportRequest, PositionEncoding,
};
use tokio::sync::{broadcast, mpsc, watch};
use typst::{
    diag::{SourceDiagnostic, SourceResult},
    layout::Position,
    syntax::{Span, VirtualPath},
    util::Deferred,
};
use typst_preview::{
    CompilationHandle, CompilationHandleImpl, CompileHost, CompileStatus, DocToSrcJumpInfo,
    EditorServer, Location, MemoryFiles, MemoryFilesShort, SourceFileServer,
};
use typst_ts_compiler::{
    service::{
        CompileActor as CompileActorInner, CompileClient as TsCompileClient,
        CompileDriver as CompileDriverInner, CompileExporter, CompileMiddleware, Compiler,
        WorkspaceProvider, WorldExporter,
    },
    vfs::notify::{FileChangeSet, MemoryEvent},
    Time, TypstSystemWorld,
};
use typst_ts_core::{
    config::CompileOpts, debug_loc::SourceSpanOffset, error::prelude::*, typst::prelude::EcoVec,
    Bytes, Error, ImmutPath, TypstDocument, TypstWorld,
};

use crate::actor::render::RenderActorRequest;
use crate::ConstConfig;

type CompileService<H> = CompileActorInner<Reporter<CompileExporter<CompileDriver>, H>>;
type CompileClient<H> = TsCompileClient<CompileService<H>>;

type DiagnosticsSender = mpsc::UnboundedSender<(String, Option<DiagnosticsMap>)>;

#[allow(clippy::too_many_arguments)]
pub fn create_server(
    diag_group: String,
    cfg: &ConstConfig,
    roots: Vec<PathBuf>,
    opts: CompileOpts,
    entry: Option<PathBuf>,
    diag_tx: DiagnosticsSender,
    doc_sender: watch::Sender<Option<Arc<TypstDocument>>>,
    render_tx: broadcast::Sender<RenderActorRequest>,
) -> Deferred<CompileActor> {
    let cfg = cfg.clone();
    let current_runtime = tokio::runtime::Handle::current();
    Deferred::new(move || {
        let compiler_driver = CompileDriver::new(roots.clone(), opts, entry.clone());
        let root = compiler_driver.inner.world.root.as_ref().to_owned();
        let handler: CompileHandler = compiler_driver.handler.clone();

        let driver = CompileExporter::new(compiler_driver).with_exporter(Box::new(
            move |_w: &dyn TypstWorld, doc| {
                let _ = doc_sender.send(Some(doc));
                // todo: is it right that ignore zero broadcast receiver?
                let _ = render_tx.send(RenderActorRequest::Render);

                Ok(())
            },
        ));
        let driver = Reporter {
            diag_group: diag_group.clone(),
            position_encoding: cfg.position_encoding,
            diag_tx,
            inner: driver,
            cb: handler.clone(),
        };
        let driver = CompileActorInner::new(driver, root).with_watch(true);

        let (server, client) = driver.split();

        current_runtime.spawn(server.spawn());

        let this = CompileActor::new(diag_group, cfg.position_encoding, handler, client);

        // todo: less bug-prone code
        if let Some(entry) = entry {
            this.entry.lock().unwrap().replace(entry.into());
        }

        this
    })
}

macro_rules! query_state {
    ($self:ident, $method:ident, $req:expr) => {{
        let doc = $self.handler.result.lock().unwrap().clone().ok();
        let enc = $self.position_encoding;
        let res = $self.steal_world(move |w| $req.request(w, doc, enc));
        res.map(CompilerQueryResponse::$method)
    }};
}

macro_rules! query_world {
    ($self:ident, $method:ident, $req:expr) => {{
        let enc = $self.position_encoding;
        let res = $self.steal_world(move |w| $req.request(w, enc));
        res.map(CompilerQueryResponse::$method)
    }};
}

#[derive(Clone)]
pub struct CompileHandler {
    result: Arc<SyncMutex<Result<Arc<TypstDocument>, CompileStatus>>>,
    inner: Arc<SyncMutex<Option<CompilationHandleImpl>>>,
}

impl CompilationHandle for CompileHandler {
    fn status(&self, status: CompileStatus) {
        let inner = self.inner.lock().unwrap();
        if let Some(inner) = inner.as_ref() {
            inner.status(status);
        }
    }

    fn notify_compile(&self, result: Result<Arc<TypstDocument>, CompileStatus>) {
        *self.result.lock().unwrap() = result.clone();

        let inner = self.inner.lock().unwrap();
        if let Some(inner) = inner.as_ref() {
            inner.notify_compile(result.clone());
        }
    }
}

pub struct CompileDriver {
    inner: CompileDriverInner,
    roots: Vec<PathBuf>,
    handler: CompileHandler,
}

impl CompileMiddleware for CompileDriver {
    type Compiler = CompileDriverInner;

    fn inner(&self) -> &Self::Compiler {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Self::Compiler {
        &mut self.inner
    }
}

impl CompileDriver {
    pub fn new(roots: Vec<PathBuf>, opts: CompileOpts, entry: Option<PathBuf>) -> Self {
        let world = TypstSystemWorld::new(opts).expect("incorrect options");
        let mut driver = CompileDriverInner::new(world);

        driver.entry_file = "detached.typ".into();
        // todo: suitable approach to avoid panic
        driver.notify_fs_event(typst_ts_compiler::vfs::notify::FilesystemEvent::Update(
            typst_ts_compiler::vfs::notify::FileChangeSet::new_inserts(vec![(
                driver.world.root.join("detached.typ").into(),
                Ok((Time::now(), Bytes::from("".as_bytes()))).into(),
            )]),
        ));

        let mut this = Self {
            inner: driver,
            roots,
            handler: CompileHandler {
                result: Arc::new(SyncMutex::new(Err(CompileStatus::Compiling))),
                inner: Arc::new(SyncMutex::new(None)),
            },
        };

        if let Some(entry) = entry {
            this.set_entry_file(entry);
        }

        this
    }

    // todo: determine root
    fn set_entry_file(&mut self, entry: PathBuf) {
        let _ = &self.roots;
        // let candidates = self
        //     .current
        //     .iter()
        //     .filter_map(|(root, package)| Some((root,
        // package.uri_to_vpath(uri).ok()?)))     .inspect(|(package_root,
        // path)| trace!(%package_root, ?path, %uri, "considering
        // candidate for full id"));

        // // Our candidates are projects containing a URI, so we expect to get
        // a set of // subdirectories. The "best" is the "most
        // specific", that is, the project that is a // subdirectory of
        // the rest. This should have the longest length.
        // let (best_package_root, best_path) =
        //     candidates.max_by_key(|(_, path)|
        // path.as_rootless_path().components().count())?;

        // let package_id = PackageId::new_current(best_package_root.clone());
        // let full_file_id = FullFileId::new(package_id, best_path);

        self.inner.set_entry_file(entry);
    }
}

pub struct Reporter<C, H> {
    diag_group: String,
    position_encoding: PositionEncoding,
    diag_tx: DiagnosticsSender,
    inner: C,
    cb: H,
}

impl<C: Compiler<World = TypstSystemWorld>, H: CompilationHandle> CompileMiddleware
    for Reporter<C, H>
{
    type Compiler = C;

    fn inner(&self) -> &Self::Compiler {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Self::Compiler {
        &mut self.inner
    }

    fn wrap_compile(
        &mut self,
        env: &mut typst_ts_compiler::service::CompileEnv,
    ) -> SourceResult<Arc<TypstDocument>> {
        self.cb.status(CompileStatus::Compiling);
        match self.inner_mut().compile(env) {
            Ok(doc) => {
                self.cb.notify_compile(Ok(doc.clone()));

                self.push_diagnostics(EcoVec::new());
                Ok(doc)
            }
            Err(err) => {
                self.cb.notify_compile(Err(CompileStatus::CompileError));

                self.push_diagnostics(err);
                Err(EcoVec::new())
            }
        }
    }
}

impl<C: Compiler + WorldExporter, H> WorldExporter for Reporter<C, H> {
    fn export(&mut self, output: Arc<typst::model::Document>) -> SourceResult<()> {
        self.inner.export(output)
    }
}

impl<C: Compiler<World = TypstSystemWorld>, H> Reporter<C, H> {
    fn push_diagnostics(&mut self, diagnostics: EcoVec<SourceDiagnostic>) {
        trace!("send diagnostics: {:#?}", diagnostics);

        // todo encoding
        let diagnostics = tinymist_query::convert_diagnostics(
            self.inner.world(),
            diagnostics.as_ref(),
            self.position_encoding,
        );

        // todo: better way to remove diagnostics
        // todo: check all errors in this file

        let main = self.inner.world().main;
        let valid = main.is_some_and(|e| e.vpath() != &VirtualPath::new("detached.typ"));

        let err = self
            .diag_tx
            .send((self.diag_group.clone(), valid.then_some(diagnostics)));
        if let Err(err) = err {
            error!("failed to send diagnostics: {:#}", err);
        }
    }
}

pub struct CompileActor {
    diag_group: String,
    position_encoding: PositionEncoding,
    handler: CompileHandler,
    entry: Arc<SyncMutex<Option<ImmutPath>>>,
    pub inner: Mutex<CompileClient<CompileHandler>>,
}

// todo: remove unsafe impl send
/// SAFETY:
/// This is safe because the not send types are only used in compiler time
/// hints.
unsafe impl Send for CompileActor {}
/// SAFETY:
/// This is safe because the not sync types are only used in compiler time
/// hints.
unsafe impl Sync for CompileActor {}

impl CompileActor {
    fn inner(&mut self) -> &mut CompileClient<CompileHandler> {
        self.inner.get_mut()
    }

    /// Steal the compiler thread and run the given function.
    pub fn steal<Ret: Send + 'static>(
        &self,
        f: impl FnOnce(&mut CompileService<CompileHandler>) -> Ret + Send + 'static,
    ) -> ZResult<Ret> {
        self.inner.lock().steal(f)
    }

    // todo: stop main
    pub fn disable(&self) {
        let res = self.steal(move |compiler| {
            let path = Path::new("detached.typ");
            let root = compiler.compiler.world().workspace_root();

            let driver = &mut compiler.compiler.compiler.inner.compiler;
            driver.set_entry_file(path.to_owned());

            // todo: suitable approach to avoid panic
            driver.notify_fs_event(typst_ts_compiler::vfs::notify::FilesystemEvent::Update(
                typst_ts_compiler::vfs::notify::FileChangeSet::new_inserts(vec![(
                    root.join("detached.typ").into(),
                    Ok((Time::now(), Bytes::from("".as_bytes()))).into(),
                )]),
            ));
        });
        if let Err(err) = res {
            error!("failed to disable main: {:#}", err);
        }
    }

    pub fn change_entry(&self, path: ImmutPath) -> Result<(), Error> {
        if !path.is_absolute() {
            return Err(error_once!("entry file must be absolute", path: path.display()));
        }

        // todo: more robust rollback logic
        let entry = self.entry.clone();
        let should_change = {
            let mut entry = entry.lock().unwrap();
            let should_change = entry.as_ref().map(|e| e != &path).unwrap_or(true);
            let prev = entry.clone();
            *entry = Some(path.clone());

            should_change.then_some(prev)
        };

        if let Some(prev) = should_change {
            let next = path.clone();

            debug!(
                "the entry file of TypstActor({}) is changed to {}",
                self.diag_group,
                next.display()
            );

            let res = self.steal(move |compiler| {
                let root = compiler.compiler.world().workspace_root();
                if !path.starts_with(&root) {
                    warn!("entry file is not in workspace root {}", path.display());
                    return;
                }

                let driver = &mut compiler.compiler.compiler.inner.compiler;
                driver.set_entry_file(path.as_ref().to_owned());
            });

            if res.is_err() {
                let mut entry = entry.lock().unwrap();
                if *entry == Some(next) {
                    *entry = prev;
                }

                return res;
            }

            // todo: trigger recompile
            let files = FileChangeSet::new_inserts(vec![]);
            let inner = self.inner.lock();
            inner.add_memory_changes(MemoryEvent::Update(files))
        }

        Ok(())
    }
}

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
                error!("TypstActor: failed to resolve span and offset: {:#}", err);
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
        let files = FileChangeSet::new_removes(files.files.into_iter().map(From::from).collect());
        self.inner().add_memory_changes(MemoryEvent::Update(files));

        Ok(())
    }
}

impl CompileHost for CompileActor {}

impl CompileActor {
    fn new(
        diag_group: String,
        position_encoding: PositionEncoding,
        handler: CompileHandler,
        inner: CompileClient<CompileHandler>,
    ) -> Self {
        Self {
            diag_group,
            position_encoding,
            handler,
            entry: Arc::new(SyncMutex::new(None)),
            inner: Mutex::new(inner),
        }
    }

    pub fn query(&self, query: CompilerQueryRequest) -> anyhow::Result<CompilerQueryResponse> {
        use CompilerQueryRequest::*;
        assert!(query.fold_feature() != FoldRequestFeature::ContextFreeUnique);

        match query {
            CompilerQueryRequest::OnSaveExport(OnSaveExportRequest { path }) => {
                self.on_save_export(path)?;
                Ok(CompilerQueryResponse::OnSaveExport(()))
            }
            Hover(req) => query_state!(self, Hover, req),
            GotoDefinition(req) => query_world!(self, GotoDefinition, req),
            InlayHint(req) => query_world!(self, InlayHint, req),
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

    fn on_save_export(&self, _path: PathBuf) -> anyhow::Result<()> {
        Ok(())
    }

    fn steal_world<T: Send + Sync + 'static>(
        &self,
        f: impl FnOnce(&TypstSystemWorld) -> T + Send + Sync + 'static,
    ) -> anyhow::Result<T> {
        let mut client = self.inner.lock();
        let fut = client.steal(move |compiler| f(compiler.compiler.world()));

        Ok(fut?)
    }
}
