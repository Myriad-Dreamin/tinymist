//! The typst actors running compilations.

use core::fmt;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex as SyncMutex},
};

use log::{debug, error, info, trace, warn};
use parking_lot::Mutex;
use tinymist_query::{
    CompilerQueryRequest, CompilerQueryResponse, DiagnosticsMap, FoldRequestFeature,
    OnExportRequest, OnSaveExportRequest, PositionEncoding,
};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
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
        CompileDriver as CompileDriverInner, CompileExporter, CompileMiddleware, Compiler,
        WorkspaceProvider, WorldExporter,
    },
    vfs::notify::{FileChangeSet, MemoryEvent},
    TypstSystemWorld,
};
use typst_ts_core::{
    config::CompileOpts, debug_loc::SourceSpanOffset, error::prelude::*, typst::prelude::EcoVec,
    Error, ImmutPath, TypstDocument, TypstWorld,
};

use super::compile::CompileClient as TsCompileClient;
use super::{compile::CompileActor as CompileActorInner, render::PdfExportConfig};
use crate::ConstConfig;
use crate::{
    actor::render::{PdfPathVars, RenderActorRequest},
    utils,
};

type CompileService<H> = CompileActorInner<Reporter<CompileExporter<CompileDriver>, H>>;
type CompileClient<H> = TsCompileClient<CompileService<H>>;

type DiagnosticsSender = mpsc::UnboundedSender<(String, Option<DiagnosticsMap>)>;

#[allow(clippy::too_many_arguments)]
pub fn create_server(
    diag_group: String,
    cfg: &ConstConfig,
    opts: CompileOpts,
    entry: Option<PathBuf>,
    snapshot: FileChangeSet,
    diag_tx: DiagnosticsSender,
    doc_sender: watch::Sender<Option<Arc<TypstDocument>>>,
    render_tx: broadcast::Sender<RenderActorRequest>,
) -> CompileActor {
    info!(
        "TypstActor: creating server for {} with arguments {:#?}",
        diag_group,
        ShowOpts(&opts)
    );

    let pos_encoding = cfg.position_encoding;
    let root: ImmutPath = opts.root_dir.as_path().into();
    let entry = entry.as_ref().map(|e| e.as_path().into());

    let handler = CompileHandler {
        result: Arc::new(SyncMutex::new(Err(CompileStatus::Compiling))),
        inner: Arc::new(SyncMutex::new(None)),
    };

    let inner = Deferred::new({
        let current_runtime = tokio::runtime::Handle::current();
        let root = root.clone();
        let diag_group = diag_group.clone();
        let handler = handler.clone();
        let entry = entry.clone();
        let render_tx = render_tx.clone();

        move || {
            // todo: entry is PathBuf, which is inefficient
            let compiler_driver = CompileDriver::new(opts, entry.clone(), handler);
            let handler: CompileHandler = compiler_driver.handler.clone();

            let ontyped_render_tx = render_tx.clone();
            let driver = CompileExporter::new(compiler_driver).with_exporter(Box::new(
                move |_w: &dyn TypstWorld, doc| {
                    let _ = doc_sender.send(Some(doc));
                    // todo: is it right that ignore zero broadcast receiver?
                    let _ = ontyped_render_tx.send(RenderActorRequest::OnTyped);

                    Ok(())
                },
            ));
            let driver = Reporter {
                diag_group: diag_group.clone(),
                position_encoding: pos_encoding,
                diag_tx,
                inner: driver,
                cb: handler.clone(),
            };
            let driver =
                CompileActorInner::new(driver, root.clone(), entry.clone()).with_watch(true);

            let (server, client) = driver.split();

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
        root,
        entry,
        pos_encoding,
        handler,
        inner,
        render_tx,
    )
}

struct ShowOpts<'a>(&'a CompileOpts);

impl<'a> fmt::Debug for ShowOpts<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompileOpts")
            .field("root_dir", &self.0.root_dir)
            .field("entry", &self.0.entry)
            .field("inputs", &self.0.inputs)
            .field("font_paths", &self.0.font_paths)
            .field("no_system_fonts", &self.0.no_system_fonts)
            .finish()
    }
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
    pub fn new(opts: CompileOpts, entry: Option<ImmutPath>, handler: CompileHandler) -> Self {
        let world = TypstSystemWorld::new(opts).expect("incorrect options");
        let driver = CompileDriverInner::new(world);

        let mut this = Self {
            inner: driver,
            handler,
        };

        if let Some(entry) = entry {
            this.set_entry_file(entry.as_ref().to_owned());
        }

        this
    }

    // todo: determine root
    fn set_entry_file(&mut self, entry: PathBuf) {
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

                self.notify_diagnostics(EcoVec::new());
                Ok(doc)
            }
            Err(err) => {
                self.cb.notify_compile(Err(CompileStatus::CompileError));

                self.notify_diagnostics(err);
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
    fn push_diagnostics(&mut self, diagnostics: Option<DiagnosticsMap>) {
        let err = self.diag_tx.send((self.diag_group.clone(), diagnostics));
        if let Err(err) = err {
            error!("failed to send diagnostics: {:#}", err);
        }
    }

    fn notify_diagnostics(&mut self, diagnostics: EcoVec<SourceDiagnostic>) {
        trace!("notify diagnostics: {:#?}", diagnostics);

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

        self.push_diagnostics(valid.then_some(diagnostics));
    }
}

pub struct CompileActor {
    diag_group: String,
    position_encoding: PositionEncoding,
    handler: CompileHandler,
    root: ImmutPath,
    entry: Arc<Mutex<Option<ImmutPath>>>,
    pub inner: Deferred<CompileClient<CompileHandler>>,
    render_tx: broadcast::Sender<RenderActorRequest>,
}

impl CompileActor {
    fn inner(&self) -> &CompileClient<CompileHandler> {
        self.inner.wait()
    }

    /// Steal the compiler thread and run the given function.
    pub fn steal<Ret: Send + 'static>(
        &self,
        f: impl FnOnce(&mut CompileService<CompileHandler>) -> Ret + Send + 'static,
    ) -> ZResult<Ret> {
        self.inner().steal(f)
    }

    /// Steal the compiler thread and run the given function.
    pub async fn steal_async<Ret: Send + 'static>(
        &self,
        f: impl FnOnce(&mut CompileService<CompileHandler>, tokio::runtime::Handle) -> Ret
            + Send
            + 'static,
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

        // todo: more robust rollback logic
        let entry = self.entry.clone();
        let should_change = {
            let mut entry = entry.lock();
            let should_change = entry.as_deref() != path.as_deref();
            let prev = entry.clone();
            *entry = path.clone();

            should_change.then_some(prev)
        };

        if let Some(prev) = should_change {
            let next = path.clone();

            debug!(
                "the entry file of TypstActor({}) is changed to {next:?}",
                self.diag_group,
            );

            self.render_tx
                .send(RenderActorRequest::ChangeExportPath(PdfPathVars {
                    root: self.root.clone(),
                    path: next.clone(),
                }))
                .unwrap();

            // todo
            let res = self.steal(move |compiler| {
                let root = compiler.compiler.world().workspace_root();
                if path.as_ref().is_some_and(|p| !p.starts_with(&root)) {
                    warn!("entry file is not in workspace root {path:?}");
                    return;
                }

                if let Some(path) = &path {
                    let driver = &mut compiler.compiler.compiler.inner.compiler;
                    driver.set_entry_file(path.as_ref().to_owned());
                }

                compiler.change_entry(path.clone());

                if path.is_none() {
                    info!("TypstActor: removing diag");
                    compiler.compiler.compiler.push_diagnostics(None);
                }
            });

            if res.is_err() {
                self.render_tx
                    .send(RenderActorRequest::ChangeExportPath(PdfPathVars {
                        root: self.root.clone(),
                        path: prev.clone(),
                    }))
                    .unwrap();

                let mut entry = entry.lock();
                // todo: the rollback is actually not atomic
                if *entry == next {
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

    pub(crate) fn change_export_pdf(&self, config: PdfExportConfig) {
        let entry = self.entry.lock();
        let path = entry
            .as_ref()
            .map(|e| e.clone().with_extension("pdf").into());
        let _ = self
            .render_tx
            .send(RenderActorRequest::ChangeConfig(PdfExportConfig {
                substitute_pattern: config.substitute_pattern,
                root: self.root.clone(),
                path,
                mode: config.mode,
            }))
            .unwrap();
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
        root: ImmutPath,
        entry: Option<ImmutPath>,
        position_encoding: PositionEncoding,
        handler: CompileHandler,
        inner: Deferred<CompileClient<CompileHandler>>,
        render_tx: broadcast::Sender<RenderActorRequest>,
    ) -> Self {
        Self {
            diag_group,
            root,
            position_encoding,
            handler,
            entry: Arc::new(Mutex::new(entry)),
            inner,
            render_tx,
        }
    }

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

    fn steal_world<T: Send + Sync + 'static>(
        &self,
        f: impl FnOnce(&TypstSystemWorld) -> T + Send + Sync + 'static,
    ) -> anyhow::Result<T> {
        let fut = self.steal(move |compiler| f(compiler.compiler.world()));

        Ok(fut?)
    }
}
