use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex as SyncMutex},
};

use anyhow::anyhow;
use futures::future::join_all;
use log::{error, trace, warn};
use tinymist_query::{
    DiagnosticsMap, LspDiagnostic, LspRange, PositionEncoding, SemanticTokenCache,
};
use tokio::sync::{broadcast, mpsc, watch, Mutex, RwLock};
use tower_lsp::lsp_types::{
    CompletionResponse, DocumentSymbolResponse, Hover, SelectionRange,
    SemanticTokensFullDeltaResult, SemanticTokensResult, SignatureHelp, SymbolInformation,
    TextDocumentContentChangeEvent, Url,
};
use typst::diag::{FileResult, SourceDiagnostic, SourceResult};
use typst::layout::Position;
use typst::model::Document;
use typst::syntax::{Source, Span};
use typst::World;
use typst_preview::CompilationHandleImpl;
use typst_preview::{CompilationHandle, CompileStatus};
use typst_preview::{CompileHost, EditorServer, MemoryFiles, MemoryFilesShort, SourceFileServer};
use typst_preview::{DocToSrcJumpInfo, Location};
use typst_ts_compiler::service::{
    CompileActor, CompileClient as TsCompileClient, CompileDriver as CompileDriverInner,
    CompileExporter, CompileMiddleware, Compiler, WorkspaceProvider, WorldExporter,
};
use typst_ts_compiler::vfs::notify::{FileChangeSet, MemoryEvent};
use typst_ts_compiler::{Time, TypstSystemWorld};
use typst_ts_core::{
    config::CompileOpts, debug_loc::SourceSpanOffset, error::prelude::*, typst::prelude::EcoVec,
    Bytes, Error, ImmutPath,
};

use crate::actor::render::RenderActorRequest;
use crate::LspHost;
use crate::{actor::render::PdfExportActor, ConstConfig};

type CompileService<H> = CompileActor<Reporter<CompileExporter<CompileDriver>, H>>;
type CompileClient<H> = TsCompileClient<CompileService<H>>;

type DiagnosticsSender = mpsc::UnboundedSender<(String, DiagnosticsMap)>;

pub struct CompileCluster {
    position_encoding: PositionEncoding,
    memory_changes: RwLock<HashMap<Arc<Path>, MemoryFileMeta>>,
    primary: CompileNode<CompileHandler>,
    pub tokens_cache: SemanticTokenCache,
    actor: Option<CompileClusterActor>,
}

pub fn create_cluster(
    host: LspHost,
    cfg: &ConstConfig,
    roots: Vec<PathBuf>,
    opts: CompileOpts,
) -> CompileCluster {
    let (diag_tx, diag_rx) = mpsc::unbounded_channel();

    let primary = create_server(
        "primary".to_owned(),
        cfg,
        CompileDriver::new(roots.clone(), opts.clone()),
        diag_tx,
    );

    CompileCluster {
        position_encoding: cfg.position_encoding,
        memory_changes: RwLock::new(HashMap::new()),
        primary,
        tokens_cache: Default::default(),
        actor: Some(CompileClusterActor {
            host,
            diag_rx,
            diagnostics: HashMap::new(),
            affect_map: HashMap::new(),
        }),
    }
}

fn create_server(
    diag_group: String,
    cfg: &ConstConfig,
    compiler_driver: CompileDriver,
    diag_tx: DiagnosticsSender,
) -> CompileNode<CompileHandler> {
    let (doc_sender, doc_recv) = watch::channel(None);
    let (render_tx, render_rx) = broadcast::channel(1024);

    tokio::spawn(PdfExportActor::new(doc_recv, render_rx).run());

    let root = compiler_driver.inner.world.root.as_ref().to_owned();
    let handler: CompileHandler = compiler_driver.handler.clone();

    let driver = CompileExporter::new(compiler_driver).with_exporter(Box::new(
        move |_w: &dyn World, doc| {
            let _ = doc_sender.send(Some(doc));
            // todo: is it right that ignore zero broadcast receiver?
            let _ = render_tx.send(RenderActorRequest::Render);

            Ok(())
        },
    ));
    let driver = Reporter {
        diag_group,
        position_encoding: cfg.position_encoding,
        diag_tx,
        inner: driver,
        cb: handler.clone(),
    };
    let driver = CompileActor::new(driver, root).with_watch(true);

    let (server, client) = driver.split();

    tokio::spawn(server.spawn());

    CompileNode::new(cfg.position_encoding, handler, client)
}

impl CompileCluster {
    pub fn split(mut self) -> (Self, CompileClusterActor) {
        let actor = self.actor.take().expect("actor is poisoned");
        (self, actor)
    }
}

pub struct CompileClusterActor {
    host: LspHost,
    diag_rx: mpsc::UnboundedReceiver<(String, DiagnosticsMap)>,

    diagnostics: HashMap<Url, HashMap<String, Vec<LspDiagnostic>>>,
    affect_map: HashMap<String, Vec<Url>>,
}

impl CompileClusterActor {
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                Some((group, diagnostics)) = self.diag_rx.recv() => {
                    self.publish(group, diagnostics).await;
                }
            }
        }
    }

    pub async fn publish(&mut self, group: String, next_diagnostics: DiagnosticsMap) {
        let affected = self.affect_map.get_mut(&group);

        let affected = affected.map(std::mem::take);

        // Gets sources which had some diagnostic published last time, but not this
        // time. The LSP specifies that files will not have diagnostics
        // updated, including removed, without an explicit update, so we need
        // to send an empty `Vec` of diagnostics to these sources.
        let clear_list = affected
            .into_iter()
            .flatten()
            .filter(|e| !next_diagnostics.contains_key(e))
            .map(|e| (e, None))
            .collect::<Vec<_>>();
        let next_affected = next_diagnostics.keys().cloned().collect();
        // Gets touched updates
        let update_list = next_diagnostics.into_iter().map(|(x, y)| (x, Some(y)));

        let tasks = clear_list.into_iter().chain(update_list);
        let tasks = tasks.map(|(url, next)| {
            let path_diags = self.diagnostics.entry(url.clone()).or_default();
            let rest_all = path_diags
                .iter()
                .filter_map(|(g, diags)| if g != &group { Some(diags) } else { None })
                .flatten()
                .cloned();

            let next_all = next.clone().into_iter().flatten();
            let to_publish = rest_all.chain(next_all).collect();

            match next {
                Some(next) => {
                    path_diags.insert(group.clone(), next);
                }
                None => {
                    path_diags.remove(&group);
                }
            }

            self.host.publish_diagnostics(url, to_publish, None)
        });

        join_all(tasks).await;

        // We just used the cache, and won't need it again, so we can update it now
        self.affect_map.insert(group, next_affected);
    }
}

#[derive(Debug, Clone)]
struct MemoryFileMeta {
    mt: Time,
    content: Source,
}

impl CompileCluster {
    pub async fn create_source(&self, path: PathBuf, content: String) -> Result<(), Error> {
        let now = Time::now();
        let path: ImmutPath = path.into();

        self.memory_changes.write().await.insert(
            path.clone(),
            MemoryFileMeta {
                mt: now,
                content: Source::detached(content.clone()),
            },
        );

        let content: Bytes = content.as_bytes().into();

        self.primary.change_entry(path.clone()).await?;
        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_inserts(vec![(path, FileResult::Ok((now, content)).into())]);
        let iw = self.primary.inner.lock().await;
        iw.add_memory_changes(MemoryEvent::Update(files));

        Ok(())
    }

    pub async fn remove_source(&self, path: PathBuf) -> Result<(), Error> {
        let path: ImmutPath = path.into();

        self.memory_changes.write().await.remove(&path);

        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_removes(vec![path]);
        // todo: change focus
        let iw = self.primary.inner.lock().await;
        iw.add_memory_changes(MemoryEvent::Update(files));

        Ok(())
    }

    pub async fn edit_source(
        &self,
        path: PathBuf,
        content: Vec<TextDocumentContentChangeEvent>,
        position_encoding: PositionEncoding,
    ) -> Result<(), Error> {
        let now = Time::now();
        let path: ImmutPath = path.into();

        let mut memory_changes = self.memory_changes.write().await;

        let meta = memory_changes
            .get_mut(&path)
            .ok_or_else(|| error_once!("file missing", path: path.display()))?;

        for change in content {
            let replacement = change.text;
            match change.range {
                Some(lsp_range) => {
                    let range =
                        LspRange::new(lsp_range, position_encoding).into_range_on(&meta.content);
                    meta.content.edit(range, &replacement);
                }
                None => {
                    meta.content.replace(&replacement);
                }
            }
        }

        meta.mt = now;

        let snapshot = FileResult::Ok((now, meta.content.text().as_bytes().into())).into();

        drop(memory_changes);

        self.primary.change_entry(path.clone()).await?;
        let files = FileChangeSet::new_inserts(vec![(path.clone(), snapshot)]);
        let iw = self.primary.inner.lock().await;
        iw.add_memory_changes(MemoryEvent::Update(files));

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct OnSaveExportRequest {
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub enum CompilerQueryRequest {
    OnSaveExport(OnSaveExportRequest),
    Hover(tinymist_query::HoverRequest),
    Completion(tinymist_query::CompletionRequest),
    SignatureHelp(tinymist_query::SignatureHelpRequest),
    DocumentSymbol(tinymist_query::DocumentSymbolRequest),
    Symbol(tinymist_query::SymbolRequest),
    SemanticTokensFull(tinymist_query::SemanticTokensFullRequest),
    SemanticTokensDelta(tinymist_query::SemanticTokensDeltaRequest),
    SelectionRange(tinymist_query::SelectionRangeRequest),
}

#[derive(Debug, Clone)]
pub enum CompilerQueryResponse {
    OnSaveExport(()),
    Hover(Option<Hover>),
    Completion(Option<CompletionResponse>),
    SignatureHelp(Option<SignatureHelp>),
    DocumentSymbol(Option<DocumentSymbolResponse>),
    Symbol(Option<Vec<SymbolInformation>>),
    SemanticTokensFull(Option<SemanticTokensResult>),
    SemanticTokensDelta(Option<SemanticTokensFullDeltaResult>),
    SelectionRange(Option<Vec<SelectionRange>>),
}

macro_rules! query_state {
    ($self:ident, $method:ident, $req:expr) => {{
        let doc = $self.handler.result.lock().unwrap().clone().ok();
        let enc = $self.position_encoding;
        let res = $self.steal_world(move |w| $req.request(w, doc, enc)).await;
        res.map(CompilerQueryResponse::$method)
    }};
}

macro_rules! query_world {
    ($self:ident, $method:ident, $req:expr) => {{
        let enc = $self.position_encoding;
        let res = $self.steal_world(move |w| $req.request(w, enc)).await;
        res.map(CompilerQueryResponse::$method)
    }};
}

macro_rules! query_tokens_cache {
    ($self:ident, $method:ident, $req:expr) => {{
        let path: ImmutPath = $req.path.clone().into();
        let vfs = $self.memory_changes.read().await;
        let snapshot = vfs.get(&path).ok_or_else(|| anyhow!("file missing"))?;
        let source = snapshot.content.clone();

        let enc = $self.position_encoding;
        let res = $req.request(&$self.tokens_cache, source, enc);
        Ok(CompilerQueryResponse::$method(res))
    }};
}

impl CompileCluster {
    pub async fn query(
        &self,
        query: CompilerQueryRequest,
    ) -> anyhow::Result<CompilerQueryResponse> {
        use CompilerQueryRequest::*;

        match query {
            SemanticTokensFull(req) => query_tokens_cache!(self, SemanticTokensFull, req),
            SemanticTokensDelta(req) => query_tokens_cache!(self, SemanticTokensDelta, req),
            _ => self.primary.query(query).await,
        }
    }
}

#[derive(Clone)]
pub struct CompileHandler {
    result: Arc<SyncMutex<Result<Arc<Document>, CompileStatus>>>,
    inner: Arc<SyncMutex<Option<CompilationHandleImpl>>>,
}

impl CompilationHandle for CompileHandler {
    fn status(&self, status: CompileStatus) {
        let inner = self.inner.lock().unwrap();
        if let Some(inner) = inner.as_ref() {
            inner.status(status);
        }
    }

    fn notify_compile(&self, result: Result<Arc<Document>, CompileStatus>) {
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
    fn new(roots: Vec<PathBuf>, opts: CompileOpts) -> Self {
        let world = TypstSystemWorld::new(opts).expect("incorrect options");
        let driver = CompileDriverInner::new(world);

        Self {
            inner: driver,
            roots,
            handler: CompileHandler {
                result: Arc::new(SyncMutex::new(Err(CompileStatus::Compiling))),
                inner: Arc::new(SyncMutex::new(None)),
            },
        }
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
    ) -> SourceResult<Arc<Document>> {
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

        let err = self.diag_tx.send((self.diag_group.clone(), diagnostics));
        if let Err(err) = err {
            error!("failed to send diagnostics: {:#}", err);
        }
    }
}

pub struct CompileNode<H: CompilationHandle> {
    position_encoding: PositionEncoding,
    handler: CompileHandler,
    entry: Arc<SyncMutex<Option<ImmutPath>>>,
    inner: Mutex<CompileClient<H>>,
}

// todo: remove unsafe impl send
unsafe impl<H: CompilationHandle> Send for CompileNode<H> {}
unsafe impl<H: CompilationHandle> Sync for CompileNode<H> {}

impl<H: CompilationHandle> CompileNode<H> {
    fn inner(&mut self) -> &mut CompileClient<H> {
        self.inner.get_mut()
    }

    /// Steal the compiler thread and run the given function.
    pub async fn steal_async<Ret: Send + 'static>(
        &self,
        f: impl FnOnce(&mut CompileService<H>, tokio::runtime::Handle) -> Ret + Send + 'static,
    ) -> ZResult<Ret> {
        self.inner.lock().await.steal_async(f).await
    }

    async fn change_entry(&self, path: ImmutPath) -> Result<(), Error> {
        if !path.is_absolute() {
            return Err(error_once!("entry file must be absolute", path: path.display()));
        }

        let entry = self.entry.clone();
        let should_change = {
            let mut entry = entry.lock().unwrap();
            let should_change = entry.as_ref().map(|e| e != &path).unwrap_or(true);
            *entry = Some(path.clone());
            should_change
        };

        if should_change {
            self.steal_async(move |compiler, _| {
                let root = compiler.compiler.world().workspace_root();
                if !path.starts_with(&root) {
                    warn!("entry file is not in workspace root {}", path.display());
                    return;
                }

                let driver = &mut compiler.compiler.compiler.inner.compiler;
                driver.set_entry_file(path.as_ref().to_owned());
            })
            .await?;
        }

        Ok(())
    }
}

impl<H: CompilationHandle> SourceFileServer for CompileNode<H> {
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

impl<H: CompilationHandle> EditorServer for CompileNode<H> {
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

impl<H: CompilationHandle> CompileHost for CompileNode<H> {}

impl<H: CompilationHandle> CompileNode<H> {
    fn new(
        position_encoding: PositionEncoding,
        handler: CompileHandler,
        inner: CompileClient<H>,
    ) -> Self {
        Self {
            position_encoding,
            handler,
            entry: Arc::new(SyncMutex::new(None)),
            inner: Mutex::new(inner),
        }
    }

    pub async fn query(
        &self,
        query: CompilerQueryRequest,
    ) -> anyhow::Result<CompilerQueryResponse> {
        use CompilerQueryRequest::*;

        match query {
            CompilerQueryRequest::OnSaveExport(OnSaveExportRequest { path }) => {
                self.on_save_export(path).await?;
                Ok(CompilerQueryResponse::OnSaveExport(()))
            }
            Hover(req) => query_state!(self, Hover, req),
            Completion(req) => query_state!(self, Completion, req),
            SignatureHelp(req) => query_world!(self, SignatureHelp, req),
            DocumentSymbol(req) => query_world!(self, DocumentSymbol, req),
            Symbol(req) => query_world!(self, Symbol, req),
            SelectionRange(req) => query_world!(self, SelectionRange, req),
            CompilerQueryRequest::SemanticTokensDelta(..)
            | CompilerQueryRequest::SemanticTokensFull(..) => unreachable!(),
        }
    }

    async fn on_save_export(&self, _path: PathBuf) -> anyhow::Result<()> {
        Ok(())
    }

    async fn steal_world<T: Send + Sync + 'static>(
        &self,
        f: impl FnOnce(&TypstSystemWorld) -> T + Send + Sync + 'static,
    ) -> anyhow::Result<T> {
        let mut client = self.inner.lock().await;
        let fut = client.steal_async(move |compiler, _| f(compiler.compiler.world()));

        Ok(fut.await?)
    }
}
