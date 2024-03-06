use std::{
    collections::HashMap,
    iter,
    path::{Path, PathBuf},
    sync::{Arc, Mutex as SyncMutex},
};

use anyhow::anyhow;
use futures::future::join_all;
use itertools::{Format, Itertools};
use log::{error, trace, warn};
use tokio::sync::{broadcast, mpsc, watch, Mutex, RwLock};
use tower_lsp::lsp_types::{
    CompletionResponse, DiagnosticRelatedInformation, DocumentSymbolResponse, Documentation, Hover,
    Location as LspLocation, MarkupContent, MarkupKind, Position as LspPosition, SelectionRange,
    SemanticTokens, SemanticTokensDelta, SemanticTokensFullDeltaResult, SemanticTokensResult,
    SignatureHelp, SignatureInformation, SymbolInformation, SymbolKind,
    TextDocumentContentChangeEvent, Url,
};
use typst::diag::{EcoString, FileError, FileResult, SourceDiagnostic, SourceResult, Tracepoint};
use typst::foundations::{Func, ParamInfo, Value};
use typst::layout::Position;
use typst::model::Document;
use typst::syntax::{
    ast::{self, AstNode},
    FileId, LinkedNode, Source, Span, Spanned, SyntaxKind, VirtualPath,
};
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
use typst_ts_compiler::{NotifyApi, Time, TypstSystemWorld};
use typst_ts_core::{
    config::CompileOpts, debug_loc::SourceSpanOffset, error::prelude::*, typst::prelude::EcoVec,
    Bytes, DynExporter, Error, ImmutPath, TypstDocument, TypstFileId,
};

use crate::actor::render::PdfExportActor;
use crate::actor::render::RenderActorRequest;
use crate::analysis::analyze::analyze_expr;
use crate::config::PositionEncoding;
use crate::lsp::LspHost;
use crate::lsp_typst_boundary::{
    lsp_to_typst, typst_to_lsp, LspDiagnostic, LspRange, LspRawRange, LspSeverity, TypstDiagnostic,
    TypstSeverity, TypstSpan,
};
use crate::semantic_tokens::SemanticTokenCache;

type CompileService<H> = CompileActor<Reporter<CompileExporter<CompileDriver>, H>>;
type CompileClient<H> = TsCompileClient<CompileService<H>>;

type DiagnosticsSender = mpsc::UnboundedSender<(String, DiagnosticsMap)>;

type DiagnosticsMap = HashMap<Url, Vec<LspDiagnostic>>;

pub type Client = TypstClient<CompileHandler>;

pub fn create_cluster(host: LspHost, roots: Vec<PathBuf>, opts: CompileOpts) -> CompileCluster {
    //
    let (diag_tx, diag_rx) = mpsc::unbounded_channel();

    let primary = create_server(
        "primary".to_owned(),
        create_compiler(roots.clone(), opts.clone()),
        diag_tx,
    );

    CompileCluster {
        memory_changes: RwLock::new(HashMap::new()),
        primary,
        semantic_tokens_delta_cache: Default::default(),
        actor: Some(CompileClusterActor {
            host,
            diag_rx,
            diagnostics: HashMap::new(),
            affect_map: HashMap::new(),
        }),
    }
}

fn create_compiler(roots: Vec<PathBuf>, opts: CompileOpts) -> CompileDriver {
    let world = TypstSystemWorld::new(opts).expect("incorrect options");
    CompileDriver::new(world, roots)
}

fn create_server(
    diag_group: String,
    compiler_driver: CompileDriver,
    diag_tx: DiagnosticsSender,
) -> CompileNode {
    let (doc_sender, doc_recv) = watch::channel(None);
    let (render_tx, render_rx) = broadcast::channel(1024);

    let exporter: DynExporter<TypstDocument> = Box::new(move |_w: &dyn World, doc| {
        let _ = doc_sender.send(Some(doc)); // it is ok to ignore the error here
                                            // todo: is it right that ignore zero broadcast receiver?
        let _ = render_tx.send(RenderActorRequest::Render);

        Ok(())
    });

    tokio::spawn(PdfExportActor::new(doc_recv, render_rx).run());

    let handler: CompileHandler = compiler_driver.handler.clone();
    let compile_server = CompileServer::new(
        diag_group,
        compiler_driver,
        handler.clone(),
        diag_tx,
        exporter,
    );

    CompileNode::new(handler, compile_server.spawn().unwrap())
}

pub struct CompileClusterActor {
    host: LspHost,
    diag_rx: mpsc::UnboundedReceiver<(String, DiagnosticsMap)>,

    diagnostics: HashMap<Url, HashMap<String, Vec<LspDiagnostic>>>,
    affect_map: HashMap<String, Vec<Url>>,
}

pub struct CompileCluster {
    memory_changes: RwLock<HashMap<Arc<Path>, MemoryFileMeta>>,
    primary: CompileNode,
    pub semantic_tokens_delta_cache: Arc<parking_lot::RwLock<SemanticTokenCache>>,
    actor: Option<CompileClusterActor>,
}

impl CompileCluster {
    pub fn split(mut self) -> (Self, CompileClusterActor) {
        let actor = self.actor.take().expect("actor is poisoned");
        (self, actor)
    }
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

        let mut primary = self.primary.inner.lock().await;

        let content: Bytes = content.as_bytes().into();

        primary.change_entry(path.clone()).await?;
        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_inserts(vec![(path, FileResult::Ok((now, content)).into())]);
        primary
            .inner()
            .add_memory_changes(MemoryEvent::Update(files));

        Ok(())
    }

    pub async fn remove_source(&self, path: PathBuf) -> Result<(), Error> {
        let path: ImmutPath = path.into();

        self.memory_changes.write().await.remove(&path);

        let mut primary = self.primary.inner.lock().await;

        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_removes(vec![path]);
        // todo: change focus
        primary
            .inner()
            .add_memory_changes(MemoryEvent::Update(files));

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

        let mut primary = self.primary.inner.lock().await;

        primary.change_entry(path.clone()).await?;
        let files = FileChangeSet::new_inserts(vec![(path.clone(), snapshot)]);
        primary
            .inner()
            .add_memory_changes(MemoryEvent::Update(files));

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct OnSaveExportRequest {
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct HoverRequest {
    pub path: PathBuf,
    pub position: LspPosition,
    pub position_encoding: PositionEncoding,
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub path: PathBuf,
    pub position: LspPosition,
    pub position_encoding: PositionEncoding,
    pub explicit: bool,
}

#[derive(Debug, Clone)]
pub struct SignatureHelpRequest {
    pub path: PathBuf,
    pub position: LspPosition,
    pub position_encoding: PositionEncoding,
}

#[derive(Debug, Clone)]
pub struct DocumentSymbolRequest {
    pub path: PathBuf,
    pub position_encoding: PositionEncoding,
}

#[derive(Debug, Clone)]
pub struct SymbolRequest {
    pub pattern: Option<String>,
    pub position_encoding: PositionEncoding,
}

#[derive(Debug, Clone)]
pub struct SelectionRangeRequest {
    pub path: PathBuf,
    pub positions: Vec<LspPosition>,
    pub position_encoding: PositionEncoding,
}

#[derive(Debug, Clone)]
pub struct SemanticTokensFullRequest {
    pub path: PathBuf,
    pub position_encoding: PositionEncoding,
}

#[derive(Debug, Clone)]
pub struct SemanticTokensDeltaRequest {
    pub path: PathBuf,
    pub previous_result_id: String,
    pub position_encoding: PositionEncoding,
}

#[derive(Debug, Clone)]
pub enum CompilerQueryRequest {
    OnSaveExport(OnSaveExportRequest),
    Hover(HoverRequest),
    Completion(CompletionRequest),
    SignatureHelp(SignatureHelpRequest),
    DocumentSymbol(DocumentSymbolRequest),
    Symbol(SymbolRequest),
    SemanticTokensFull(SemanticTokensFullRequest),
    SemanticTokensDelta(SemanticTokensDeltaRequest),
    SelectionRange(SelectionRangeRequest),
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

impl CompileCluster {
    pub async fn query(
        &self,
        query: CompilerQueryRequest,
    ) -> anyhow::Result<CompilerQueryResponse> {
        match query {
            CompilerQueryRequest::SemanticTokensFull(SemanticTokensFullRequest {
                path,
                position_encoding,
            }) => self
                .semantic_tokens_full(path, position_encoding)
                .await
                .map(CompilerQueryResponse::SemanticTokensFull),
            CompilerQueryRequest::SemanticTokensDelta(SemanticTokensDeltaRequest {
                path,
                previous_result_id,
                position_encoding,
            }) => self
                .semantic_tokens_delta(path, previous_result_id, position_encoding)
                .await
                .map(CompilerQueryResponse::SemanticTokensDelta),
            _ => self.primary.query(query).await,
        }
    }

    async fn semantic_tokens_full(
        &self,
        path: PathBuf,
        position_encoding: PositionEncoding,
    ) -> anyhow::Result<Option<SemanticTokensResult>> {
        let path: ImmutPath = path.into();

        let source = self
            .memory_changes
            .read()
            .await
            .get(&path)
            .ok_or_else(|| anyhow!("file missing"))?
            .content
            .clone();

        let (tokens, result_id) = self.get_semantic_tokens_full(&source, position_encoding);

        Ok(Some(
            SemanticTokens {
                result_id: Some(result_id),
                data: tokens,
            }
            .into(),
        ))
    }

    async fn semantic_tokens_delta(
        &self,
        path: PathBuf,
        previous_result_id: String,
        position_encoding: PositionEncoding,
    ) -> anyhow::Result<Option<SemanticTokensFullDeltaResult>> {
        let path: ImmutPath = path.into();

        let source = self
            .memory_changes
            .read()
            .await
            .get(&path)
            .ok_or_else(|| anyhow!("file missing"))?
            .content
            .clone();

        let (tokens, result_id) = self.try_semantic_tokens_delta_from_result_id(
            &source,
            &previous_result_id,
            position_encoding,
        );

        Ok(match tokens {
            Ok(edits) => Some(
                SemanticTokensDelta {
                    result_id: Some(result_id),
                    edits,
                }
                .into(),
            ),
            Err(tokens) => Some(
                SemanticTokens {
                    result_id: Some(result_id),
                    data: tokens,
                }
                .into(),
            ),
        })
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
    fn new(world: TypstSystemWorld, roots: Vec<PathBuf>) -> Self {
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

pub struct CompileServer<H: CompilationHandle> {
    inner: CompileService<H>,
    client: TypstClient<H>,
}

pub struct Reporter<C, H> {
    diag_group: String,
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
        fn convert_diagnostics<'a>(
            project: &TypstSystemWorld,
            errors: impl IntoIterator<Item = &'a TypstDiagnostic>,
            position_encoding: PositionEncoding,
        ) -> DiagnosticsMap {
            errors
                .into_iter()
                .flat_map(|error| {
                    convert_diagnostic(project, error, position_encoding)
                        .map_err(move |conversion_err| {
                            error!("could not convert Typst error to diagnostic: {conversion_err:?} error to convert: {error:?}");
                        })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .into_group_map()
        }

        fn convert_diagnostic(
            project: &TypstSystemWorld,
            typst_diagnostic: &TypstDiagnostic,
            position_encoding: PositionEncoding,
        ) -> anyhow::Result<(Url, LspDiagnostic)> {
            let uri;
            let lsp_range;
            if let Some((id, span)) = diagnostic_span_id(typst_diagnostic) {
                uri = Url::from_file_path(project.path_for_id(id)?).unwrap();
                let source = project.source(id)?;
                lsp_range = diagnostic_range(&source, span, position_encoding).raw_range;
            } else {
                uri = Url::from_file_path(project.root.clone()).unwrap();
                lsp_range = LspRawRange::default();
            };

            let lsp_severity = diagnostic_severity(typst_diagnostic.severity);

            let typst_message = &typst_diagnostic.message;
            let typst_hints = &typst_diagnostic.hints;
            let lsp_message = format!("{typst_message}{}", diagnostic_hints(typst_hints));

            let tracepoints =
                diagnostic_related_information(project, typst_diagnostic, position_encoding)?;

            let diagnostic = LspDiagnostic {
                range: lsp_range,
                severity: Some(lsp_severity),
                message: lsp_message,
                source: Some("typst".to_owned()),
                related_information: Some(tracepoints),
                ..Default::default()
            };

            Ok((uri, diagnostic))
        }

        fn tracepoint_to_relatedinformation(
            project: &TypstSystemWorld,
            tracepoint: &Spanned<Tracepoint>,
            position_encoding: PositionEncoding,
        ) -> anyhow::Result<Option<DiagnosticRelatedInformation>> {
            if let Some(id) = tracepoint.span.id() {
                let uri = Url::from_file_path(project.path_for_id(id)?).unwrap();
                let source = project.source(id)?;

                if let Some(typst_range) = source.range(tracepoint.span) {
                    let lsp_range = typst_to_lsp::range(typst_range, &source, position_encoding);

                    return Ok(Some(DiagnosticRelatedInformation {
                        location: LspLocation {
                            uri,
                            range: lsp_range.raw_range,
                        },
                        message: tracepoint.v.to_string(),
                    }));
                }
            }

            Ok(None)
        }

        fn diagnostic_related_information(
            project: &TypstSystemWorld,
            typst_diagnostic: &TypstDiagnostic,
            position_encoding: PositionEncoding,
        ) -> anyhow::Result<Vec<DiagnosticRelatedInformation>> {
            let mut tracepoints = vec![];

            for tracepoint in &typst_diagnostic.trace {
                if let Some(info) =
                    tracepoint_to_relatedinformation(project, tracepoint, position_encoding)?
                {
                    tracepoints.push(info);
                }
            }

            Ok(tracepoints)
        }

        fn diagnostic_span_id(typst_diagnostic: &TypstDiagnostic) -> Option<(FileId, TypstSpan)> {
            iter::once(typst_diagnostic.span)
                .chain(typst_diagnostic.trace.iter().map(|trace| trace.span))
                .find_map(|span| Some((span.id()?, span)))
        }

        fn diagnostic_range(
            source: &Source,
            typst_span: TypstSpan,
            position_encoding: PositionEncoding,
        ) -> LspRange {
            // Due to #241 and maybe typst/typst#2035, we sometimes fail to find the span.
            // In that case, we use a default span as a better alternative to
            // panicking.
            //
            // This may have been fixed after Typst 0.7.0, but it's still nice to avoid
            // panics in case something similar reappears.
            match source.find(typst_span) {
                Some(node) => {
                    let typst_range = node.range();
                    typst_to_lsp::range(typst_range, source, position_encoding)
                }
                None => LspRange::new(
                    LspRawRange::new(LspPosition::new(0, 0), LspPosition::new(0, 0)),
                    position_encoding,
                ),
            }
        }

        fn diagnostic_severity(typst_severity: TypstSeverity) -> LspSeverity {
            match typst_severity {
                TypstSeverity::Error => LspSeverity::ERROR,
                TypstSeverity::Warning => LspSeverity::WARNING,
            }
        }

        fn diagnostic_hints(
            typst_hints: &[EcoString],
        ) -> Format<impl Iterator<Item = EcoString> + '_> {
            iter::repeat(EcoString::from("\n\nHint: "))
                .take(typst_hints.len())
                .interleave(typst_hints.iter().cloned())
                .format("")
        }

        // todo encoding
        let diagnostics = convert_diagnostics(
            self.inner.world(),
            diagnostics.as_ref(),
            PositionEncoding::Utf16,
        );

        trace!("send diagnostics: {:#?}", diagnostics);
        let err = self.diag_tx.send((self.diag_group.clone(), diagnostics));
        if let Err(err) = err {
            error!("failed to send diagnostics: {:#}", err);
        }
    }
}

impl<H: CompilationHandle> CompileServer<H> {
    pub fn new(
        diag_group: String,
        compiler_driver: CompileDriver,
        cb: H,
        diag_tx: DiagnosticsSender,
        exporter: DynExporter<TypstDocument>,
    ) -> Self {
        let root = compiler_driver.inner.world.root.clone();
        let driver = CompileExporter::new(compiler_driver).with_exporter(exporter);
        let driver = Reporter {
            diag_group,
            diag_tx,
            inner: driver,
            cb,
        };
        let inner = CompileActor::new(driver, root.as_ref().to_owned()).with_watch(true);

        Self {
            inner,
            client: TypstClient {
                entry: Arc::new(SyncMutex::new(None)),
                inner: once_cell::sync::OnceCell::new(),
            },
        }
    }

    pub fn spawn(self) -> Result<TypstClient<H>, Error> {
        let (server, client) = self.inner.split();
        tokio::spawn(server.spawn());

        self.client.inner.set(client).ok().unwrap();

        Ok(self.client)
    }
}

pub struct TypstClient<H: CompilationHandle> {
    entry: Arc<SyncMutex<Option<ImmutPath>>>,
    inner: once_cell::sync::OnceCell<CompileClient<H>>,
}

// todo: remove unsafe impl send
unsafe impl<H: CompilationHandle> Send for TypstClient<H> {}
unsafe impl<H: CompilationHandle> Sync for TypstClient<H> {}

impl<H: CompilationHandle> TypstClient<H> {
    fn inner(&mut self) -> &mut CompileClient<H> {
        self.inner.get_mut().unwrap()
    }

    /// Steal the compiler thread and run the given function.
    pub async fn steal_async<Ret: Send + 'static>(
        &mut self,
        f: impl FnOnce(&mut CompileService<H>, tokio::runtime::Handle) -> Ret + Send + 'static,
    ) -> ZResult<Ret> {
        self.inner().steal_async(f).await
    }

    async fn change_entry(&mut self, path: ImmutPath) -> Result<(), Error> {
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

impl<H: CompilationHandle> SourceFileServer for TypstClient<H> {
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

impl<H: CompilationHandle> EditorServer for TypstClient<H> {
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

impl<H: CompilationHandle> CompileHost for TypstClient<H> {}

pub struct CompileNode {
    handler: CompileHandler,
    inner: Arc<Mutex<Client>>,
}

impl CompileNode {
    fn new(handler: CompileHandler, unwrap: TypstClient<CompileHandler>) -> Self {
        Self {
            handler,
            inner: Arc::new(Mutex::new(unwrap)),
        }
    }

    pub async fn query(
        &self,
        query: CompilerQueryRequest,
    ) -> anyhow::Result<CompilerQueryResponse> {
        match query {
            CompilerQueryRequest::OnSaveExport(OnSaveExportRequest { path }) => {
                self.on_save_export(path).await?;
                Ok(CompilerQueryResponse::OnSaveExport(()))
            }
            CompilerQueryRequest::Hover(HoverRequest {
                path,
                position,
                position_encoding,
            }) => self
                .hover(path, position, position_encoding)
                .await
                .map(CompilerQueryResponse::Hover),
            CompilerQueryRequest::Completion(CompletionRequest {
                path,
                position,
                position_encoding,
                explicit,
            }) => self
                .completion(path, position, position_encoding, explicit)
                .await
                .map(CompilerQueryResponse::Completion),
            CompilerQueryRequest::SignatureHelp(SignatureHelpRequest {
                path,
                position,
                position_encoding,
            }) => self
                .signature_help(path, position, position_encoding)
                .await
                .map(CompilerQueryResponse::SignatureHelp),
            CompilerQueryRequest::DocumentSymbol(DocumentSymbolRequest {
                path,
                position_encoding,
            }) => self
                .document_symbol(path, position_encoding)
                .await
                .map(CompilerQueryResponse::DocumentSymbol),
            CompilerQueryRequest::Symbol(SymbolRequest {
                pattern,
                position_encoding,
            }) => self
                .symbol(pattern, position_encoding)
                .await
                .map(CompilerQueryResponse::Symbol),
            CompilerQueryRequest::SelectionRange(SelectionRangeRequest {
                path,
                positions,
                position_encoding,
            }) => self
                .selection_range(path, positions, position_encoding)
                .await
                .map(CompilerQueryResponse::SelectionRange),
            CompilerQueryRequest::SemanticTokensDelta(..)
            | CompilerQueryRequest::SemanticTokensFull(..) => unreachable!(),
        }
    }

    async fn on_save_export(&self, _path: PathBuf) -> anyhow::Result<()> {
        Ok(())
    }

    async fn hover(
        &self,
        path: PathBuf,
        position: LspPosition,
        position_encoding: PositionEncoding,
    ) -> anyhow::Result<Option<Hover>> {
        let doc = self.handler.result.lock().unwrap().clone().ok();

        let mut client = self.inner.lock().await;
        let fut = client.steal_async(move |compiler, _| {
            let world = compiler.compiler.world();

            let source = get_suitable_source_in_workspace(world, &path).ok()?;
            let typst_offset =
                lsp_to_typst::position_to_offset(position, position_encoding, &source);

            let typst_tooltip = typst_ide::tooltip(world, doc.as_deref(), &source, typst_offset)?;

            let ast_node = LinkedNode::new(source.root()).leaf_at(typst_offset)?;
            let range = typst_to_lsp::range(ast_node.range(), &source, position_encoding);

            Some(Hover {
                contents: typst_to_lsp::tooltip(&typst_tooltip),
                range: Some(range.raw_range),
            })
        });

        Ok(fut.await?)
    }

    async fn completion(
        &self,
        path: PathBuf,
        position: LspPosition,
        position_encoding: PositionEncoding,
        explicit: bool,
    ) -> anyhow::Result<Option<CompletionResponse>> {
        let doc = self.handler.result.lock().unwrap().clone().ok();

        let mut client = self.inner.lock().await;
        let fut = client.steal_async(move |compiler, _| {
            let world = compiler.compiler.world();

            let source = get_suitable_source_in_workspace(world, &path).ok()?;
            let typst_offset =
                lsp_to_typst::position_to_offset(position, position_encoding, &source);

            let (typst_start_offset, completions) =
                typst_ide::autocomplete(world, doc.as_deref(), &source, typst_offset, explicit)?;

            let lsp_start_position =
                typst_to_lsp::offset_to_position(typst_start_offset, position_encoding, &source);
            let replace_range = LspRawRange::new(lsp_start_position, position);
            Some(typst_to_lsp::completions(&completions, replace_range).into())
        });

        Ok(fut.await?)
    }

    async fn signature_help(
        &self,
        path: PathBuf,
        position: LspPosition,
        position_encoding: PositionEncoding,
    ) -> anyhow::Result<Option<SignatureHelp>> {
        fn surrounding_function_syntax<'b>(
            leaf: &'b LinkedNode,
        ) -> Option<(ast::Expr<'b>, LinkedNode<'b>, ast::Args<'b>)> {
            let parent = leaf.parent()?;
            let parent = match parent.kind() {
                SyntaxKind::Named => parent.parent()?,
                _ => parent,
            };
            let args = parent.cast::<ast::Args>()?;
            let grand = parent.parent()?;
            let expr = grand.cast::<ast::Expr>()?;
            let callee = match expr {
                ast::Expr::FuncCall(call) => call.callee(),
                ast::Expr::Set(set) => set.target(),
                _ => return None,
            };
            Some((callee, grand.find(callee.span())?, args))
        }

        fn param_index_at_leaf(
            leaf: &LinkedNode,
            function: &Func,
            args: ast::Args,
        ) -> Option<usize> {
            let deciding = deciding_syntax(leaf);
            let params = function.params()?;
            let param_index = find_param_index(&deciding, params, args)?;
            trace!("got param index {param_index}");
            Some(param_index)
        }

        /// Find the piece of syntax that decides what we're completing.
        fn deciding_syntax<'b>(leaf: &'b LinkedNode) -> LinkedNode<'b> {
            let mut deciding = leaf.clone();
            while !matches!(
                deciding.kind(),
                SyntaxKind::LeftParen | SyntaxKind::Comma | SyntaxKind::Colon
            ) {
                let Some(prev) = deciding.prev_leaf() else {
                    break;
                };
                deciding = prev;
            }
            deciding
        }

        fn find_param_index(
            deciding: &LinkedNode,
            params: &[ParamInfo],
            args: ast::Args,
        ) -> Option<usize> {
            match deciding.kind() {
                // After colon: "func(param:|)", "func(param: |)".
                SyntaxKind::Colon => {
                    let prev = deciding.prev_leaf()?;
                    let param_ident = prev.cast::<ast::Ident>()?;
                    params
                        .iter()
                        .position(|param| param.name == param_ident.as_str())
                }
                // Before: "func(|)", "func(hi|)", "func(12,|)".
                SyntaxKind::Comma | SyntaxKind::LeftParen => {
                    let next = deciding.next_leaf();
                    let following_param = next.as_ref().and_then(|next| next.cast::<ast::Ident>());
                    match following_param {
                        Some(next) => params
                            .iter()
                            .position(|param| param.named && param.name.starts_with(next.as_str())),
                        None => {
                            let positional_args_so_far = args
                                .items()
                                .filter(|arg| matches!(arg, ast::Arg::Pos(_)))
                                .count();
                            params
                                .iter()
                                .enumerate()
                                .filter(|(_, param)| param.positional)
                                .map(|(i, _)| i)
                                .nth(positional_args_so_far)
                        }
                    }
                }
                _ => None,
            }
        }

        fn markdown_docs(docs: &str) -> Documentation {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: docs.to_owned(),
            })
        }

        let mut client = self.inner.lock().await;
        let fut = client.steal_async(move |compiler, _| {
            let world = compiler.compiler.world();

            let source = get_suitable_source_in_workspace(world, &path).ok()?;
            let typst_offset =
                lsp_to_typst::position_to_offset(position, position_encoding, &source);

            let ast_node = LinkedNode::new(source.root()).leaf_at(typst_offset)?;
            let (callee, callee_node, args) = surrounding_function_syntax(&ast_node)?;

            let mut ancestor = &ast_node;
            while !ancestor.is::<ast::Expr>() {
                ancestor = ancestor.parent()?;
            }

            if !callee.hash() && !matches!(callee, ast::Expr::MathIdent(_)) {
                return None;
            }

            let values = analyze_expr(world, &callee_node);

            let function = values.into_iter().find_map(|v| match v {
                Value::Func(f) => Some(f),
                _ => None,
            })?;
            trace!("got function {function:?}");

            let param_index = param_index_at_leaf(&ast_node, &function, args);

            let label = format!(
                "{}({}){}",
                function.name().unwrap_or("<anonymous closure>"),
                match function.params() {
                    Some(params) => params
                        .iter()
                        .map(typst_to_lsp::param_info_to_label)
                        .join(", "),
                    None => "".to_owned(),
                },
                match function.returns() {
                    Some(returns) => format!("-> {}", typst_to_lsp::cast_info_to_label(returns)),
                    None => "".to_owned(),
                }
            );
            let params = function
                .params()
                .unwrap_or_default()
                .iter()
                .map(typst_to_lsp::param_info)
                .collect();
            trace!("got signature info {label} {params:?}");

            let documentation = function.docs().map(markdown_docs);

            let active_parameter = param_index.map(|i| i as u32);

            Some(SignatureInformation {
                label,
                documentation,
                parameters: Some(params),
                active_parameter,
            })
        });

        let signature = fut.await?;

        Ok(signature.map(|signature| SignatureHelp {
            signatures: vec![signature],
            active_signature: Some(0),
            active_parameter: None,
        }))
    }

    async fn document_symbol(
        &self,
        path: PathBuf,
        position_encoding: PositionEncoding,
    ) -> anyhow::Result<Option<DocumentSymbolResponse>> {
        let mut client = self.inner.lock().await;
        let fut = client.steal_async(move |compiler, _| {
            let world = compiler.compiler.world();

            let source = get_suitable_source_in_workspace(world, &path).ok()?;

            let uri = Url::from_file_path(path).unwrap();
            let symbols = get_document_symbols(source, uri, position_encoding);

            symbols.map(DocumentSymbolResponse::Flat)
        });

        Ok(fut.await?)
    }

    async fn symbol(
        &self,
        pattern: Option<String>,
        position_encoding: PositionEncoding,
    ) -> anyhow::Result<Option<Vec<SymbolInformation>>> {
        let mut client = self.inner.lock().await;
        let fut = client.steal_async(move |compiler, _| {
            let world = compiler.compiler.world();

            // todo: expose source

            let mut symbols = vec![];

            world.iter_dependencies(&mut |path, _| {
                let Ok(source) = get_suitable_source_in_workspace(world, path) else {
                    return;
                };
                let uri = Url::from_file_path(path).unwrap();
                let res =
                    get_document_symbols(source, uri, position_encoding).and_then(|symbols| {
                        pattern
                            .as_ref()
                            .map(|pattern| filter_document_symbols(symbols, pattern))
                    });

                if let Some(mut res) = res {
                    symbols.append(&mut res)
                }
            });

            Some(symbols)
        });

        Ok(fut.await?)
    }

    async fn selection_range(
        &self,
        path: PathBuf,
        positions: Vec<LspPosition>,
        position_encoding: PositionEncoding,
    ) -> anyhow::Result<Option<Vec<SelectionRange>>> {
        fn range_for_node(
            source: &Source,
            position_encoding: PositionEncoding,
            node: &LinkedNode,
        ) -> SelectionRange {
            let range = typst_to_lsp::range(node.range(), source, position_encoding);
            SelectionRange {
                range: range.raw_range,
                parent: node
                    .parent()
                    .map(|node| Box::new(range_for_node(source, position_encoding, node))),
            }
        }

        let mut client = self.inner.lock().await;
        let fut = client.steal_async(move |compiler, _| {
            let world = compiler.compiler.world();

            let source = get_suitable_source_in_workspace(world, &path).ok()?;

            let mut ranges = Vec::new();
            for position in positions {
                let typst_offset =
                    lsp_to_typst::position_to_offset(position, position_encoding, &source);
                let tree = LinkedNode::new(source.root());
                let leaf = tree.leaf_at(typst_offset)?;
                ranges.push(range_for_node(&source, position_encoding, &leaf));
            }

            Some(ranges)
        });

        Ok(fut.await?)
    }
}

fn get_suitable_source_in_workspace(w: &TypstSystemWorld, p: &Path) -> FileResult<Source> {
    // todo: source in packages
    let relative_path = p
        .strip_prefix(&w.workspace_root())
        .map_err(|_| FileError::NotFound(p.to_owned()))?;
    w.source(TypstFileId::new(None, VirtualPath::new(relative_path)))
}

fn filter_document_symbols(
    symbols: Vec<SymbolInformation>,
    query_string: &str,
) -> Vec<SymbolInformation> {
    symbols
        .into_iter()
        .filter(|e| e.name.contains(query_string))
        .collect()
}

#[comemo::memoize]
fn get_document_symbols(
    source: Source,
    uri: Url,
    position_encoding: PositionEncoding,
) -> Option<Vec<SymbolInformation>> {
    struct DocumentSymbolWorker {
        symbols: Vec<SymbolInformation>,
    }

    impl DocumentSymbolWorker {
        /// Get all symbols for a node recursively.
        pub fn get_symbols<'a>(
            &mut self,
            node: LinkedNode<'a>,
            source: &'a Source,
            uri: &'a Url,
            position_encoding: PositionEncoding,
        ) -> anyhow::Result<()> {
            let own_symbol = get_ident(&node, source, uri, position_encoding)?;

            for child in node.children() {
                self.get_symbols(child, source, uri, position_encoding)?;
            }

            if let Some(symbol) = own_symbol {
                self.symbols.push(symbol);
            }

            Ok(())
        }
    }

    /// Get symbol for a leaf node of a valid type, or `None` if the node is an
    /// invalid type.
    #[allow(deprecated)]
    fn get_ident(
        node: &LinkedNode,
        source: &Source,
        uri: &Url,
        position_encoding: PositionEncoding,
    ) -> anyhow::Result<Option<SymbolInformation>> {
        match node.kind() {
            SyntaxKind::Label => {
                let ast_node = node
                    .cast::<ast::Label>()
                    .ok_or_else(|| anyhow!("cast to ast node failed: {:?}", node))?;
                let name = ast_node.get().to_string();
                let symbol = SymbolInformation {
                    name,
                    kind: SymbolKind::CONSTANT,
                    tags: None,
                    deprecated: None, // do not use, deprecated, use `tags` instead
                    location: LspLocation {
                        uri: uri.clone(),
                        range: typst_to_lsp::range(node.range(), source, position_encoding)
                            .raw_range,
                    },
                    container_name: None,
                };
                Ok(Some(symbol))
            }
            SyntaxKind::Ident => {
                let ast_node = node
                    .cast::<ast::Ident>()
                    .ok_or_else(|| anyhow!("cast to ast node failed: {:?}", node))?;
                let name = ast_node.get().to_string();
                let Some(parent) = node.parent() else {
                    return Ok(None);
                };
                let kind = match parent.kind() {
                    // for variable definitions, the Let binding holds an Ident
                    SyntaxKind::LetBinding => SymbolKind::VARIABLE,
                    // for function definitions, the Let binding holds a Closure which holds the
                    // Ident
                    SyntaxKind::Closure => {
                        let Some(grand_parent) = parent.parent() else {
                            return Ok(None);
                        };
                        match grand_parent.kind() {
                            SyntaxKind::LetBinding => SymbolKind::FUNCTION,
                            _ => return Ok(None),
                        }
                    }
                    _ => return Ok(None),
                };
                let symbol = SymbolInformation {
                    name,
                    kind,
                    tags: None,
                    deprecated: None, // do not use, deprecated, use `tags` instead
                    location: LspLocation {
                        uri: uri.clone(),
                        range: typst_to_lsp::range(node.range(), source, position_encoding)
                            .raw_range,
                    },
                    container_name: None,
                };
                Ok(Some(symbol))
            }
            SyntaxKind::Markup => {
                let name = node.get().to_owned().into_text().to_string();
                if name.is_empty() {
                    return Ok(None);
                }
                let Some(parent) = node.parent() else {
                    return Ok(None);
                };
                let kind = match parent.kind() {
                    SyntaxKind::Heading => SymbolKind::NAMESPACE,
                    _ => return Ok(None),
                };
                let symbol = SymbolInformation {
                    name,
                    kind,
                    tags: None,
                    deprecated: None, // do not use, deprecated, use `tags` instead
                    location: LspLocation {
                        uri: uri.clone(),
                        range: typst_to_lsp::range(node.range(), source, position_encoding)
                            .raw_range,
                    },
                    container_name: None,
                };
                Ok(Some(symbol))
            }
            _ => Ok(None),
        }
    }

    let root = LinkedNode::new(source.root());

    let mut worker = DocumentSymbolWorker { symbols: vec![] };

    let res = worker
        .get_symbols(root, &source, &uri, position_encoding)
        .ok();

    res.map(|_| worker.symbols)
}
