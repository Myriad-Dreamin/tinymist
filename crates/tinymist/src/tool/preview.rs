//! Document preview tool for Typst

#![allow(missing_docs)]

use std::num::NonZeroUsize;
use std::sync::LazyLock;
use std::{collections::HashMap, net::SocketAddr, path::Path, sync::Arc};

use futures::{SinkExt, StreamExt, TryStreamExt};
use hyper::header::HeaderValue;
use hyper::service::service_fn;
use hyper_tungstenite::{tungstenite::Message, HyperWebsocket, HyperWebsocketStream};
use hyper_util::rt::TokioIo;
use hyper_util::server::graceful::GracefulShutdown;
use lsp_types::notification::Notification;
use lsp_types::Url;
use parking_lot::Mutex;
use reflexo_typst::debug_loc::SourceSpanOffset;
use reflexo_typst::Bytes;
use reflexo_typst::{error::prelude::*, Error};
use serde::Serialize;
use serde_json::Value as JsonValue;
use sync_lsp::just_ok;
use tinymist_assets::TYPST_PREVIEW_HTML;
use tinymist_std::error::IgnoreLogging;
use tinymist_std::typst::TypstDocument;
use tokio::sync::{mpsc, oneshot};
use typst::layout::{Abs, Frame, FrameItem, Point, Position, Size};
use typst::syntax::{LinkedNode, Source, Span, SyntaxKind};
use typst::visualize::Geometry;
use typst::World;
use typst_preview::{
    frontend_html, ControlPlaneMessage, ControlPlaneResponse, ControlPlaneRx, ControlPlaneTx,
    DocToSrcJumpInfo, EditorServer, Location, MemoryFiles, MemoryFilesShort, PreviewArgs,
    PreviewBuilder, PreviewMode, Previewer, WsMessage,
};
use typst_shim::syntax::LinkedNodeExt;

use crate::project::{
    CompileHandlerImpl, CompileServerOpts, LspCompiledArtifact, LspInterrupt, LspWorld,
    ProjectClient, ProjectCompiler, ProjectInsId, ProjectState, WorldProvider,
};
use crate::*;
use actor::preview::{PreviewActor, PreviewRequest, PreviewTab};
use project::world::vfs::{notify::MemoryEvent, FileChangeSet};
use project::{watch_deps, ProjectPreviewState};

pub use typst_preview::CompileStatus;

pub enum PreviewKind {
    Regular,
    Browsing,
    Background,
}

impl ServerState {
    pub fn background_preview(&mut self) {
        if !self.config.preview.background.enabled {
            return;
        }

        let args = self.config.preview.background.args.clone();
        let args = args.unwrap_or_else(|| {
            vec![
                "--data-plane-host=127.0.0.1:23635".to_string(),
                "--invert-colors=auto".to_string(),
            ]
        });

        let res = self.start_preview_inner(args, PreviewKind::Background);

        // todo: looks ugly
        self.client.handle.spawn(async move {
            let fut = match res {
                Ok(fut) => fut,
                Err(e) => {
                    log::error!("failed to start background preview: {e:?}");
                    return;
                }
            };
            tokio::pin!(fut);
            let () = fut.as_mut().await;

            if let Some(Err(e)) = fut.as_mut().take_output() {
                log::error!("failed to start background preview: {e:?}");
            }
        });
    }

    /// Start a preview instance.
    pub fn start_preview_inner(
        &mut self,
        cli_args: Vec<String>,
        kind: PreviewKind,
    ) -> SchedulableResponse<crate::tool::preview::StartPreviewResponse> {
        use std::path::Path;

        use crate::tool::preview::PreviewCliArgs;
        use clap::Parser;

        // clap parse
        let cli_args = ["preview"]
            .into_iter()
            .chain(cli_args.iter().map(|e| e.as_str()));
        let cli_args =
            PreviewCliArgs::try_parse_from(cli_args).map_err(|e| invalid_params(e.to_string()))?;

        // todo: preview specific arguments are not used
        let entry = cli_args.compile.input.as_ref();
        let entry = entry
            .map(|input| {
                let input = Path::new(&input);
                if !input.is_absolute() {
                    // std::env::current_dir().unwrap().join(input)
                    return Err(invalid_params("entry file must be absolute path"));
                };

                Ok(input.into())
            })
            .transpose()?;

        let task_id = cli_args.preview.task_id.clone();
        if task_id == "primary" {
            return Err(invalid_params("task id 'primary' is reserved"));
        }

        if cli_args.not_as_primary && matches!(kind, PreviewKind::Background) {
            return Err(invalid_params(
                "cannot start background preview as non-primary",
            ));
        }

        let previewer = typst_preview::PreviewBuilder::new(cli_args.preview.clone());
        let watcher = previewer.compile_watcher();

        let primary = &mut self.project.compiler.primary;
        // todo: recover pin status reliably
        let is_browsing = matches!(kind, PreviewKind::Browsing | PreviewKind::Background);
        let is_background = matches!(kind, PreviewKind::Background);

        let registered_as_primary = !cli_args.not_as_primary
            && (is_browsing || entry.is_some())
            && self.preview.watchers.register(&primary.id, watcher);
        if matches!(kind, PreviewKind::Background) && !registered_as_primary {
            return Err(invalid_params(
                "failed to register background preview to the primary instance",
            ));
        }

        if registered_as_primary {
            let id = primary.id.clone();

            if let Some(entry) = entry {
                self.change_main_file(Some(entry)).map_err(internal_error)?;
            }
            self.set_pin_by_preview(true, is_browsing);

            self.preview
                .start(cli_args, previewer, id, true, is_background)
        } else if let Some(entry) = entry {
            let id = self
                .restart_dedicate(&task_id, Some(entry))
                .map_err(internal_error)?;

            if !self.project.preview.register(&id, watcher) {
                return Err(invalid_params(
                    "cannot register preview to the compiler instance",
                ));
            }

            self.preview
                .start(cli_args, previewer, id, false, is_background)
        } else {
            return Err(internal_error("entry file must be provided"));
        }
    }
}
/// The preview's view of the compiled artifact.
pub struct PreviewCompileView {
    /// The artifact and snap.
    pub snap: LspCompiledArtifact,
}

impl typst_preview::CompileView for PreviewCompileView {
    fn doc(&self) -> Option<TypstDocument> {
        self.snap.doc.clone().ok()
    }

    fn status(&self) -> CompileStatus {
        match self.snap.doc {
            Ok(_) => CompileStatus::CompileSuccess,
            Err(_) => CompileStatus::CompileError,
        }
    }

    fn is_on_saved(&self) -> bool {
        self.snap.signal.by_fs_events
    }

    fn is_by_entry_update(&self) -> bool {
        self.snap.signal.by_entry_update
    }

    fn resolve_source_span(&self, loc: Location) -> Option<SourceSpanOffset> {
        let world = &self.snap.world;
        let Location::Src(loc) = loc;

        let source_id = world.id_for_path(Path::new(&loc.filepath))?;

        let source = world.source(source_id).ok()?;
        let cursor =
            source.line_column_to_byte(loc.pos.line as usize, loc.pos.character as usize)?;

        let node = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
        if !matches!(node.kind(), SyntaxKind::Text | SyntaxKind::MathText) {
            return None;
        }
        let span = node.span();
        // todo: unicode char
        let offset = cursor.saturating_sub(node.offset());

        Some(SourceSpanOffset { span, offset })
    }

    // todo: use vec2bbox to handle bbox correctly
    fn resolve_frame_loc(
        &self,
        pos: &reflexo::debug_loc::DocumentPosition,
    ) -> Option<(SourceSpanOffset, SourceSpanOffset)> {
        let TypstDocument::Paged(doc) = self.doc()? else {
            return None;
        };
        let world = &self.snap.world;

        let page = pos.page_no.checked_sub(1)?;
        let page = doc.pages.get(page)?;

        let click = Point::new(Abs::pt(pos.x as f64), Abs::pt(pos.y as f64));
        jump_from_click(world, &page.frame, click)
    }

    fn resolve_document_position(&self, loc: Location) -> Vec<Position> {
        let world = &self.snap.world;
        let Location::Src(src_loc) = loc;

        let line = src_loc.pos.line as usize;
        let column = src_loc.pos.character as usize;

        let doc = self.snap.success_doc();
        let Some(doc) = doc.as_ref() else {
            return vec![];
        };

        let Some(source_id) = world.id_for_path(Path::new(&src_loc.filepath)) else {
            return vec![];
        };
        let Some(source) = world.source(source_id).ok() else {
            return vec![];
        };
        let Some(cursor) = source.line_column_to_byte(line, column) else {
            return vec![];
        };

        jump_from_cursor(doc, &source, cursor)
    }

    fn resolve_span(&self, span: Span, offset: Option<usize>) -> Option<DocToSrcJumpInfo> {
        let world = &self.snap.world;
        let resolve_off =
            |src: &Source, off: usize| src.byte_to_line(off).zip(src.byte_to_column(off));

        let source = world.source(span.id()?).ok()?;
        let mut range = source.find(span)?.range();
        if let Some(off) = offset {
            if off < range.len() {
                range.start += off;
            }
        }
        let filepath = world.path_for_id(span.id()?).ok()?.to_err().ok()?;
        Some(DocToSrcJumpInfo {
            filepath: filepath.to_string_lossy().to_string(),
            start: resolve_off(&source, range.start),
            end: resolve_off(&source, range.end),
        })
    }
}

/// CLI Arguments for the preview tool.
#[derive(Debug, Clone, clap::Parser)]
pub struct PreviewCliArgs {
    /// Preview arguments
    #[clap(flatten)]
    pub preview: PreviewArgs,

    /// Compile arguments
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// Preview mode
    #[clap(long = "preview-mode", default_value = "document", value_name = "MODE")]
    pub preview_mode: PreviewMode,

    /// Data plane server will bind to this address. Note: if it equals to
    /// `static_file_host`, same address will be used.
    #[clap(
        long = "data-plane-host",
        default_value = "127.0.0.1:23625",
        value_name = "HOST",
        hide(true)
    )]
    pub data_plane_host: String,

    /// Control plane server will bind to this address
    #[clap(
        long = "control-plane-host",
        default_value = "127.0.0.1:23626",
        value_name = "HOST",
        hide(true)
    )]
    pub control_plane_host: String,

    /// (Deprecated) (File) Host for the preview server. Note: if it equals to
    /// `data_plane_host`, same address will be used.
    #[clap(
        long = "host",
        value_name = "HOST",
        default_value = "",
        alias = "static-file-host"
    )]
    pub static_file_host: String,

    /// Let it not be the primary instance.
    #[clap(long = "not-primary", hide(true))]
    pub not_as_primary: bool,

    /// Open the preview in the browser after compilation. If `--no-open` is
    /// set, this flag will be ignored.
    #[clap(long = "open")]
    pub open: bool,

    /// Don't open the preview in the browser after compilation. If `--open` is
    /// set as well, this flag will win.
    #[clap(long = "no-open")]
    pub no_open: bool,
}

impl PreviewCliArgs {
    pub fn open_in_browser(&self, default: bool) -> bool {
        !self.no_open && (self.open || default)
    }
}

/// The global state of the preview tool.
pub struct PreviewState {
    /// Connection to the LSP client.
    client: TypedLspClient<PreviewState>,
    /// The backend running actor.
    preview_tx: mpsc::UnboundedSender<PreviewRequest>,
    /// the watchers for the preview
    pub(crate) watchers: ProjectPreviewState,
}

impl PreviewState {
    /// Create a new preview state.
    pub fn new(watchers: ProjectPreviewState, client: TypedLspClient<PreviewState>) -> Self {
        let (preview_tx, preview_rx) = mpsc::unbounded_channel();

        client.handle.spawn(
            PreviewActor {
                client: client.clone().to_untyped(),
                tabs: HashMap::default(),
                preview_rx,
                watchers: watchers.clone(),
            }
            .run(),
        );

        Self {
            client,
            preview_tx,
            watchers,
        }
    }

    pub(crate) fn stop_all(&mut self) {
        log::info!("Stopping all preview tasks");

        let mut watchers = self.watchers.inner.lock();
        for (_, watcher) in watchers.iter_mut() {
            self.preview_tx
                .send(PreviewRequest::Kill(
                    watcher.task_id().to_owned(),
                    oneshot::channel().0,
                ))
                .log_error_with(|| format!("failed to send kill request({:?})", watcher.task_id()));
        }
        watchers.clear();
    }
}

/// Response for starting a preview.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPreviewResponse {
    static_server_port: Option<u16>,
    static_server_addr: Option<String>,
    data_plane_port: Option<u16>,
    is_primary: bool,
}

pub struct PreviewProjectHandler {
    pub project_id: ProjectInsId,
    client: Box<dyn ProjectClient>,
}

impl PreviewProjectHandler {
    pub fn flush_compile(&self) {
        let _ = self.project_id;
        self.client
            .interrupt(LspInterrupt::Compile(self.project_id.clone()));
    }

    pub fn settle(&self) -> Result<(), Error> {
        self.client
            .interrupt(LspInterrupt::Settle(self.project_id.clone()));
        Ok(())
    }

    pub fn unpin_primary(&self) {
        self.client.server_event(ServerEvent::UnpinPrimaryByPreview);
    }
}

impl EditorServer for PreviewProjectHandler {
    async fn update_memory_files(
        &self,
        files: MemoryFiles,
        reset_shadow: bool,
    ) -> Result<(), Error> {
        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_inserts(
            files
                .files
                .into_iter()
                .map(|(path, content)| {
                    // todo: cloning PathBuf -> Arc<Path>
                    (path.into(), Ok(Bytes::from_string(content)).into())
                })
                .collect(),
        );

        let intr = LspInterrupt::Memory(if reset_shadow {
            MemoryEvent::Sync(files)
        } else {
            MemoryEvent::Update(files)
        });
        self.client.interrupt(intr);

        Ok(())
    }

    async fn remove_shadow_files(&self, files: MemoryFilesShort) -> Result<(), Error> {
        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_removes(files.files.into_iter().map(From::from).collect());
        self.client
            .interrupt(LspInterrupt::Memory(MemoryEvent::Update(files)));

        Ok(())
    }
}

impl PreviewState {
    /// Start a preview on a given compiler.
    pub fn start(
        &self,
        args: PreviewCliArgs,
        previewer: PreviewBuilder,
        // compile_handler: Arc<CompileHandler>,
        project_id: ProjectInsId,
        is_primary: bool,
        is_background: bool,
    ) -> SchedulableResponse<StartPreviewResponse> {
        let compile_handler = Arc::new(PreviewProjectHandler {
            project_id,
            client: Box::new(self.client.clone().to_untyped()),
        });

        let task_id = args.preview.task_id.clone();
        let open_in_browser = args.open_in_browser(false);
        log::info!("PreviewTask({task_id}): arguments: {args:#?}");

        if !args.static_file_host.is_empty() && (args.static_file_host != args.data_plane_host) {
            return Err(internal_error("--static-file-host is removed"));
        }

        let (lsp_tx, lsp_rx) = ControlPlaneTx::new(false);
        let ControlPlaneRx {
            resp_rx,
            ctl_tx,
            mut shutdown_rx,
        } = lsp_rx;

        let (websocket_tx, websocket_rx) = mpsc::unbounded_channel();

        let previewer = previewer.build(lsp_tx, compile_handler.clone());

        // Forward preview responses to lsp client
        let tid = task_id.clone();
        let client = self.client.clone();
        self.client.handle.spawn(async move {
            let mut resp_rx = resp_rx;
            while let Some(resp) = resp_rx.recv().await {
                use ControlPlaneResponse::*;

                match resp {
                    // ignoring compile status per task.
                    CompileStatus(..) => {}
                    SyncEditorChanges(..) => {
                        log::warn!("PreviewTask({tid}): is sending SyncEditorChanges in lsp mode");
                    }
                    EditorScrollTo(s) => client.send_notification::<ScrollSource>(&s),
                    Outline(s) => client.send_notification::<NotifDocumentOutline>(&s),
                }
            }

            log::info!("PreviewTask({tid}): response channel closed");
        });

        // Process preview shutdown
        let tid = task_id.clone();
        let preview_tx = self.preview_tx.clone();
        self.client.handle.spawn(async move {
            // shutdown_rx
            let Some(()) = shutdown_rx.recv().await else {
                return;
            };

            log::info!("PreviewTask({tid}): internal killing");
            let (tx, rx) = oneshot::channel();
            preview_tx.send(PreviewRequest::Kill(tid.clone(), tx)).ok();
            rx.await.ok();
            log::info!("PreviewTask({tid}): internal killed");
        });

        let preview_tx = self.preview_tx.clone();
        just_future(async move {
            let mut previewer = previewer.await;
            bind_streams(&mut previewer, websocket_rx);

            // Put a fence to ensure the previewer can receive the first compilation.
            // The fence must be put after the previewer is initialized.
            compile_handler.flush_compile();

            // Replace the data plane port in the html to self
            let frontend_html = frontend_html(TYPST_PREVIEW_HTML, args.preview_mode, "/");

            let srv = make_http_server(frontend_html, args.data_plane_host, websocket_tx).await;
            let addr = srv.addr;
            log::info!("PreviewTask({task_id}): preview server listening on: {addr}");

            let resp = StartPreviewResponse {
                static_server_port: Some(addr.port()),
                static_server_addr: Some(addr.to_string()),
                data_plane_port: Some(addr.port()),
                is_primary,
            };

            if open_in_browser {
                open::that_detached(format!("http://127.0.0.1:{}", addr.port()))
                    .log_error("failed to open browser for preview");
            }

            let sent = preview_tx.send(PreviewRequest::Started(PreviewTab {
                task_id,
                previewer,
                srv,
                ctl_tx,
                compile_handler,
                is_primary,
                is_background,
            }));
            sent.map_err(|_| internal_error("failed to register preview tab"))?;

            Ok(resp)
        })
    }

    /// Kill a preview task. Ignore if the task is not found.
    pub fn kill(&self, task_id: String) -> AnySchedulableResponse {
        let (tx, rx) = oneshot::channel();

        let sent = self.preview_tx.send(PreviewRequest::Kill(task_id, tx));
        sent.map_err(|_| internal_error("failed to send kill request"))?;

        just_future(async move { rx.await.map_err(|_| internal_error("cancelled"))? })
    }

    /// Scroll the preview to a given position.
    pub fn scroll(&self, task_id: String, req: ControlPlaneMessage) -> AnySchedulableResponse {
        let sent = self.preview_tx.send(PreviewRequest::Scroll(task_id, req));
        sent.map_err(|_| internal_error("failed to send scroll request"))?;

        just_ok(JsonValue::Null)
    }
}

/// created by `make_http_server`
pub struct HttpServer {
    /// The address the server is listening on.
    pub addr: SocketAddr,
    /// The sender to shutdown the server.
    pub shutdown_tx: oneshot::Sender<()>,
    /// The join handle of the server.
    pub join: tokio::task::JoinHandle<()>,
}

/// Create a http server for the previewer.
pub async fn make_http_server(
    frontend_html: String,
    static_file_addr: String,
    websocket_tx: mpsc::UnboundedSender<HyperWebsocket>,
) -> HttpServer {
    use http_body_util::Full;
    use hyper::body::{Bytes, Incoming};
    type Server = hyper_util::server::conn::auto::Builder<hyper_util::rt::TokioExecutor>;

    let listener = tokio::net::TcpListener::bind(&static_file_addr)
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    log::info!("preview server listening on http://{addr}");

    let frontend_html = hyper::body::Bytes::from(frontend_html);
    let make_service = move || {
        let frontend_html = frontend_html.clone();
        let websocket_tx = websocket_tx.clone();
        let static_file_addr = static_file_addr.clone();
        service_fn(move |mut req: hyper::Request<Incoming>| {
            let frontend_html = frontend_html.clone();
            let websocket_tx = websocket_tx.clone();
            let static_file_addr = static_file_addr.clone();
            async move {
                // When a user visits a website in a browser, that website can try to connect to
                // our http / websocket server on `127.0.0.1` which may leak sensitive
                // information. We could use CORS headers to explicitly disallow
                // this. However, for Websockets, this does not work. Thus, we
                // manually check the `Origin` header. Browsers always send this
                // header for cross-origin requests.
                //
                // Important: This does _not_ protect against malicious users that share the
                // same computer as us (i.e. multi- user systems where the users
                // don't trust each other). In this case, malicious attackers can _still_
                // connect to our http / websocket servers (using a browser and
                // otherwise). And additionally they can impersonate a tinymist
                // http / websocket server towards a legitimate frontend/html client.
                // This requires additional protection that may be added in the future.
                let origin_header = req.headers().get("Origin");
                if origin_header
                    .is_some_and(|h| !is_valid_origin(h, &static_file_addr, addr.port()))
                {
                    anyhow::bail!(
                        "Connection with unexpected `Origin` header. Closing connection."
                    );
                }

                // Check if the request is a websocket upgrade request.
                if hyper_tungstenite::is_upgrade_request(&req) {
                    if origin_header.is_none() {
                        log::error!("websocket connection is not set `Origin` header, which will be a hard error in the future.");
                    }

                    let Some((response, websocket)) = hyper_tungstenite::upgrade(&mut req, None)
                        .log_error("Error in websocket upgrade")
                    else {
                        anyhow::bail!("cannot upgrade as websocket connection");
                    };

                    let _ = websocket_tx.send(websocket);

                    // Return the response so the spawned future can continue.
                    Ok(response)
                } else if req.uri().path() == "/" {
                    // log::debug!("Serve frontend: {mode:?}");
                    let res = hyper::Response::builder()
                        .header(hyper::header::CONTENT_TYPE, "text/html")
                        .body(Full::<Bytes>::from(frontend_html))
                        .unwrap();
                    Ok(res)
                } else {
                    // jump to /
                    let res = hyper::Response::builder()
                        .status(hyper::StatusCode::FOUND)
                        .header(hyper::header::LOCATION, "/")
                        .body(Full::<Bytes>::default())
                        .unwrap();
                    Ok(res)
                }
            }
        })
    };

    let (shutdown_tx, rx) = tokio::sync::oneshot::channel();
    let (final_tx, final_rx) = tokio::sync::oneshot::channel();

    // the graceful watcher
    let graceful = hyper_util::server::graceful::GracefulShutdown::new();

    let serve_conn = move |server: &Server, graceful: &GracefulShutdown, conn| {
        let (stream, _peer_addr) = match conn {
            Ok(conn) => conn,
            Err(e) => {
                log::error!("accept error: {e}");
                return;
            }
        };

        let conn = server.serve_connection_with_upgrades(TokioIo::new(stream), make_service());
        let conn = graceful.watch(conn.into_owned());
        tokio::spawn(async move {
            conn.await.log_error("cannot serve http");
        });
    };

    let join = tokio::spawn(async move {
        // when this signal completes, start shutdown
        let mut signal = std::pin::pin!(final_rx);

        let mut server = Server::new(hyper_util::rt::TokioExecutor::new());
        server.http1().keep_alive(true);

        loop {
            tokio::select! {
                conn = listener.accept() => serve_conn(&server, &graceful, conn),
                Ok(_) = &mut signal => {
                    log::info!("graceful shutdown signal received");
                    break;
                }
            }
        }

        tokio::select! {
            _ = graceful.shutdown() => {
                log::info!("Gracefully shutdown!");
            },
            _ = tokio::time::sleep(reflexo::time::Duration::from_secs(10)) => {
                log::info!("Waited 10 seconds for graceful shutdown, aborting...");
            }
        }
    });
    tokio::spawn(async move {
        let _ = rx.await;
        final_tx.send(()).ok();
        log::info!("Preview server joined");
    });

    HttpServer {
        addr,
        shutdown_tx,
        join,
    }
}

fn is_valid_origin(h: &HeaderValue, static_file_addr: &str, expected_port: u16) -> bool {
    static GITPOD_ID_AND_HOST: LazyLock<Option<(String, String)>> = LazyLock::new(|| {
        let workspace_id = std::env::var("GITPOD_WORKSPACE_ID").ok();
        let cluster_host = std::env::var("GITPOD_WORKSPACE_CLUSTER_HOST").ok();
        workspace_id.zip(cluster_host)
    });

    is_valid_origin_impl(h, static_file_addr, expected_port, &GITPOD_ID_AND_HOST)
}

// Separate function so we can do gitpod-related tests without relying on env
// vars.
fn is_valid_origin_impl(
    origin_header: &HeaderValue,
    static_file_addr: &str,
    expected_port: u16,
    gitpod_id_and_host: &Option<(String, String)>,
) -> bool {
    let Ok(Ok(origin_url)) = origin_header.to_str().map(Url::parse) else {
        return false;
    };

    // Path is not allowed in Origin headers
    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Origin
    if origin_url.path() != "/" && origin_url.path() != "" {
        return false;
    };

    let expected_origin = {
        let expected_host = Url::parse(&format!("http://{static_file_addr}")).unwrap();
        let expected_host = expected_host.host_str().unwrap();
        // Don't take the port from `static_file_addr` (it may have a dummy port e.g.
        // `127.0.0.1:0`)
        format!("http://{expected_host}:{expected_port}")
    };

    let gitpod_expected_origin = gitpod_id_and_host
        .as_ref()
        .map(|(workspace_id, cluster_host)| {
            format!("https://{expected_port}-{workspace_id}.{cluster_host}")
        });

    *origin_header == expected_origin
        // tmistele (PR #1382): The VSCode webview panel needs an exception: It doesn't send `http://{static_file_addr}`
        // as `Origin`. Instead it sends `vscode-webview://<random>`. Thus, we allow any
        // `Origin` starting with `vscode-webview://` as well. I think that's okay from a security
        // point of view, because I think malicious websites can't trick browsers into sending
        // `vscode-webview://...` as `Origin`.
        || origin_url.scheme() == "vscode-webview"
        // `code-server` also needs an exception: It opens `http://localhost:8080/proxy/<port>` in
        // the browser and proxies requests through to tinymist (which runs at `127.0.0.1:<port>`).
        // Thus, the `Origin` header will be `http://localhost:8080` which doesn't match what
        // we expect. Thus, just always allow anything from localhost/127.0.0.1
        // https://github.com/Myriad-Dreamin/tinymist/issues/1350
        || (
            matches!(origin_url.host_str(), Some("localhost") | Some("127.0.0.1"))
            && origin_url.scheme() == "http"
        )
        // `gitpod` also needs an exception. It loads `https://<port>-<workspace>.<host>` in the browser
        // and proxies requests through to tinymist (which runs as `127.0.0.1:<port>`).
        // We can detect this by looking at the env variables (see `GITPOD_ID_AND_HOST` in `is_valid_origin(..)`)
        || gitpod_expected_origin.is_some_and(|o| o == *origin_header)
}

/// Entry point of the preview tool.
pub async fn preview_main(args: PreviewCliArgs) -> Result<()> {
    log::info!("Arguments: {args:#?}");
    let handle = tokio::runtime::Handle::current();

    let open_in_browser = args.open_in_browser(true);
    let static_file_host =
        if args.static_file_host == args.data_plane_host || !args.static_file_host.is_empty() {
            Some(args.static_file_host)
        } else {
            None
        };

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        log::info!("Ctrl-C received, exiting");
        std::process::exit(0);
    });

    let verse = args.compile.resolve()?;
    let previewer = PreviewBuilder::new(args.preview);

    let (service, handle) = {
        // type EditorSender = mpsc::UnboundedSender<EditorRequest>;
        let (editor_tx, mut editor_rx) = mpsc::unbounded_channel();
        let (intr_tx, intr_rx) = tokio::sync::mpsc::unbounded_channel();

        // todo: unify filesystem watcher
        let (dep_tx, dep_rx) = tokio::sync::mpsc::unbounded_channel();
        let fs_intr_tx = intr_tx.clone();
        tokio::spawn(watch_deps(dep_rx, move |event| {
            fs_intr_tx.interrupt(LspInterrupt::Fs(event));
        }));

        // Consume editor_rx
        tokio::spawn(async move { while editor_rx.recv().await.is_some() {} });

        let preview_state = ProjectPreviewState::default();
        let config = Config::default();

        // Create the actor
        let compile_handle = Arc::new(CompileHandlerImpl {
            preview: preview_state.clone(),
            is_standalone: true,
            export: crate::task::ExportTask::new(handle, None, config.export()),
            editor_tx,
            client: Box::new(intr_tx.clone()),
            analysis: Arc::default(),

            status_revision: Mutex::default(),
            notified_revision: Mutex::default(),
        });

        let mut compiler = ProjectCompiler::new(
            verse,
            dep_tx,
            CompileServerOpts {
                handler: compile_handle,
                enable_watch: true,
            },
        );
        let registered = preview_state.register(&compiler.primary.id, previewer.compile_watcher());
        if !registered {
            tinymist_std::bail!("failed to register preview");
        }

        let handle: Arc<PreviewProjectHandler> = Arc::new(PreviewProjectHandler {
            project_id: compiler.primary.id.clone(),
            client: Box::new(intr_tx),
        });

        compiler.primary.reason.by_entry_update = true;
        let service = async move {
            let handler = compiler.handler.clone();
            handler.on_any_compile_reason(&mut compiler);

            let mut intr_rx = intr_rx;
            while let Some(intr) = intr_rx.recv().await {
                log::debug!("Project compiler received: {intr:?}");
                ProjectState::do_interrupt(&mut compiler, intr);
            }

            log::info!("Project compiler exited");
        };

        (service, handle)
    };

    let (lsp_tx, mut lsp_rx) = ControlPlaneTx::new(true);

    let control_plane_server_handle = tokio::spawn(async move {
        let (control_sock_tx, mut control_sock_rx) = mpsc::unbounded_channel();

        let srv =
            make_http_server(String::default(), args.control_plane_host, control_sock_tx).await;
        log::info!("Control panel server listening on: {}", srv.addr);

        let control_websocket = control_sock_rx.recv().await.unwrap();
        let ws = control_websocket.await.unwrap();

        tokio::pin!(ws);

        loop {
            tokio::select! {
                Some(resp) = lsp_rx.resp_rx.recv() => {
                    let r = ws
                        .send(Message::Text(serde_json::to_string(&resp).unwrap()))
                        .await;
                    let Err(err) = r else {
                        continue;
                    };

                    log::warn!("failed to send response to editor {err:?}");
                    break;

                }
                msg = ws.next() => {
                    let msg = match msg {
                        Some(Ok(Message::Text(msg))) => Some(msg),
                        Some(Ok(msg)) => {
                            log::error!("unsupported message: {msg:?}");
                            break;
                        }
                        Some(Err(e)) => {
                            log::error!("failed to receive message: {e}");
                            break;
                        }
                        _ => None,
                    };

                    if let Some(msg) = msg {
                        let Ok(msg) = serde_json::from_str::<ControlPlaneMessage>(&msg) else {
                            log::warn!("failed to parse control plane request: {msg:?}");
                            break;
                        };

                        lsp_rx.ctl_tx.send(msg).unwrap();
                    } else {
                        // todo: inform the editor that the connection is closed.
                        break;
                    }
                }

            }
        }

        let _ = srv.shutdown_tx.send(());
        let _ = srv.join.await;
    });

    let (websocket_tx, websocket_rx) = mpsc::unbounded_channel();
    let mut previewer = previewer.build(lsp_tx, handle.clone()).await;
    tokio::spawn(service);

    bind_streams(&mut previewer, websocket_rx);

    let frontend_html = frontend_html(TYPST_PREVIEW_HTML, args.preview_mode, "/");

    let static_server = if let Some(static_file_host) = static_file_host {
        log::warn!("--static-file-host is deprecated, which will be removed in the future. Use --data-plane-host instead.");
        let html = frontend_html.clone();
        Some(make_http_server(html, static_file_host, websocket_tx.clone()).await)
    } else {
        None
    };

    let srv = make_http_server(frontend_html, args.data_plane_host, websocket_tx).await;
    log::info!("Data plane server listening on: {}", srv.addr);

    let static_server_addr = static_server.as_ref().map(|s| s.addr).unwrap_or(srv.addr);
    log::info!("Static file server listening on: {static_server_addr}");

    if open_in_browser {
        open::that_detached(format!("http://{static_server_addr}"))
            .log_error("failed to open browser for preview");
    }

    let _ = tokio::join!(previewer.join(), srv.join, control_plane_server_handle);
    // Assert that the static server's lifetime is longer than the previewer.
    let _s = static_server;

    Ok(())
}

struct ScrollSource;

impl Notification for ScrollSource {
    type Params = DocToSrcJumpInfo;
    const METHOD: &'static str = "tinymist/preview/scrollSource";
}

struct NotifDocumentOutline;

impl Notification for NotifDocumentOutline {
    type Params = typst_preview::Outline;
    const METHOD: &'static str = "tinymist/documentOutline";
}

/// Determine where to jump to based on a click in a frame.
pub fn jump_from_click(
    world: &LspWorld,
    frame: &Frame,
    click: Point,
) -> Option<(SourceSpanOffset, SourceSpanOffset)> {
    // Try to find a link first.
    for (pos, item) in frame.items() {
        if let FrameItem::Link(_dest, size) = item {
            if is_in_rect(*pos, *size, click) {
                // todo: url reaction
                return None;
            }
        }
    }

    // If there's no link, search for a jump target.
    for (mut pos, item) in frame.items().rev() {
        match item {
            FrameItem::Group(group) => {
                // TODO: Handle transformation.
                if let Some(span) = jump_from_click(world, &group.frame, click - pos) {
                    return Some(span);
                }
            }

            FrameItem::Text(text) => {
                for glyph in &text.glyphs {
                    let width = glyph.x_advance.at(text.size);
                    if is_in_rect(
                        Point::new(pos.x, pos.y - text.size),
                        Size::new(width, text.size),
                        click,
                    ) {
                        let (span, span_offset) = glyph.span;
                        let mut span_offset = span_offset as usize;
                        let Some(id) = span.id() else { continue };
                        let source = world.source(id).ok()?;
                        let node = source.find(span)?;
                        if matches!(node.kind(), SyntaxKind::Text | SyntaxKind::MathText)
                            && (click.x - pos.x) > width / 2.0
                        {
                            span_offset += glyph.range().len();
                        }

                        let span_offset = SourceSpanOffset {
                            span,
                            offset: span_offset,
                        };

                        return Some((span_offset, span_offset));
                    }

                    pos.x += width;
                }
            }

            FrameItem::Shape(shape, span) => {
                let Geometry::Rect(size) = shape.geometry else {
                    continue;
                };
                if is_in_rect(pos, size, click) {
                    let span = (*span).into();
                    return Some((span, span));
                }
            }

            FrameItem::Image(_, size, span) if is_in_rect(pos, *size, click) => {
                let span = (*span).into();
                return Some((span, span));
            }

            _ => {}
        }
    }

    None
}

/// Find the output location in the document for a cursor position.
fn jump_from_cursor(document: &TypstDocument, source: &Source, cursor: usize) -> Vec<Position> {
    let Some(node) = LinkedNode::new(source.root())
        .leaf_at_compat(cursor)
        .filter(|node| node.kind() == SyntaxKind::Text)
    else {
        return vec![];
    };

    let mut p = Point::default();

    let span = node.span();
    match document {
        TypstDocument::Paged(paged_doc) => {
            let mut positions: Vec<Position> = vec![];
            for (i, page) in paged_doc.pages.iter().enumerate() {
                let mut min_dis = u64::MAX;
                if let Some(pos) = find_in_frame(&page.frame, span, &mut min_dis, &mut p) {
                    if let Some(page) = NonZeroUsize::new(i + 1) {
                        positions.push(Position { page, point: pos });
                    }
                }
            }

            log::info!("jump_from_cursor: {positions:#?}");

            positions
        }
        _ => vec![],
    }
}

/// Find the position of a span in a frame.
fn find_in_frame(frame: &Frame, span: Span, min_dis: &mut u64, p: &mut Point) -> Option<Point> {
    for (mut pos, item) in frame.items() {
        if let FrameItem::Group(group) = item {
            // TODO: Handle transformation.
            if let Some(point) = find_in_frame(&group.frame, span, min_dis, p) {
                return Some(point + pos);
            }
        }

        if let FrameItem::Text(text) = item {
            for glyph in &text.glyphs {
                if glyph.span.0 == span {
                    return Some(pos);
                }
                if glyph.span.0.id() == span.id() {
                    let dis = glyph
                        .span
                        .0
                        .into_raw()
                        .get()
                        .abs_diff(span.into_raw().get());
                    if dis < *min_dis {
                        *min_dis = dis;
                        *p = pos;
                    }
                }
                pos.x += glyph.x_advance.at(text.size);
            }
        }
    }

    None
}

/// Whether a rectangle with the given size at the given position contains the
/// click position.
fn is_in_rect(pos: Point, size: Size, click: Point) -> bool {
    pos.x <= click.x && pos.x + size.x >= click.x && pos.y <= click.y && pos.y + size.y >= click.y
}

fn bind_streams(previewer: &mut Previewer, websocket_rx: mpsc::UnboundedReceiver<HyperWebsocket>) {
    previewer.start_data_plane(
        websocket_rx,
        |conn: Result<HyperWebsocketStream, hyper_tungstenite::tungstenite::Error>| {
            let conn = conn.map_err(error_once_map_string!("cannot receive websocket"))?;

            Ok(conn
                .sink_map_err(|e| error_once!("cannot serve_with websocket", err: e.to_string()))
                .map_err(|e| error_once!("cannot serve_with websocket", err: e.to_string()))
                .with(|msg| {
                    Box::pin(async move {
                        let msg = match msg {
                            WsMessage::Text(msg) => Message::Text(msg),
                            WsMessage::Binary(msg) => Message::Binary(msg),
                        };
                        Ok(msg)
                    })
                })
                .map_ok(|msg| match msg {
                    Message::Text(msg) => WsMessage::Text(msg),
                    Message::Binary(msg) => WsMessage::Binary(msg),
                    _ => WsMessage::Text("unsupported message".to_owned()),
                }))
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_origin(origin: &'static str, static_file_addr: &str, port: u16) -> bool {
        is_valid_origin(&HeaderValue::from_static(origin), static_file_addr, port)
    }

    #[test]
    fn test_valid_origin_localhost() {
        assert!(check_origin("http://127.0.0.1:42", "127.0.0.1:42", 42));
        assert!(check_origin("http://127.0.0.1:42", "127.0.0.1:42", 42));
        assert!(check_origin("http://127.0.0.1:42", "127.0.0.1:0", 42));
        assert!(check_origin("http://localhost:42", "127.0.0.1:42", 42));
        assert!(check_origin("http://localhost:42", "127.0.0.1:0", 42));
        assert!(check_origin("http://localhost", "127.0.0.1:0", 42));

        assert!(check_origin("http://127.0.0.1:42", "localhost:42", 42));
        assert!(check_origin("http://127.0.0.1:42", "localhost:42", 42));
        assert!(check_origin("http://127.0.0.1:42", "localhost:0", 42));
        assert!(check_origin("http://localhost:42", "localhost:42", 42));
        assert!(check_origin("http://localhost:42", "localhost:0", 42));
        assert!(check_origin("http://localhost", "localhost:0", 42));
    }

    #[test]
    fn test_invalid_origin_localhost() {
        assert!(!check_origin("https://huh.io:8080", "127.0.0.1:42", 42));
        assert!(!check_origin("http://huh.io:8080", "127.0.0.1:42", 42));
        assert!(!check_origin("https://huh.io:443", "127.0.0.1:42", 42));
        assert!(!check_origin("http://huh.io:42", "127.0.0.1:0", 42));
        assert!(!check_origin("http://huh.io", "127.0.0.1:42", 42));
        assert!(!check_origin("https://huh.io", "127.0.0.1:42", 42));

        assert!(!check_origin("https://huh.io:8080", "localhost:42", 42));
        assert!(!check_origin("http://huh.io:8080", "localhost:42", 42));
        assert!(!check_origin("https://huh.io:443", "localhost:42", 42));
        assert!(!check_origin("http://huh.io:42", "localhost:0", 42));
        assert!(!check_origin("http://huh.io", "localhost:42", 42));
        assert!(!check_origin("https://huh.io", "localhost:42", 42));
    }

    #[test]
    fn test_invalid_origin_scheme() {
        assert!(!check_origin("ftp://127.0.0.1:42", "127.0.0.1:42", 42));
        assert!(!check_origin("ftp://localhost:42", "127.0.0.1:42", 42));
        assert!(!check_origin("ftp://127.0.0.1:42", "127.0.0.1:0", 42));
        assert!(!check_origin("ftp://localhost:42", "127.0.0.1:0", 42));

        // The scheme must be specified.
        assert!(!check_origin("127.0.0.1:42", "127.0.0.1:0", 42));
        assert!(!check_origin("localhost:42", "127.0.0.1:0", 42));
        assert!(!check_origin("localhost:42", "127.0.0.1:42", 42));
        assert!(!check_origin("127.0.0.1:42", "127.0.0.1:42", 42));
    }

    #[test]
    fn test_valid_origin_vscode() {
        assert!(check_origin("vscode-webview://it", "127.0.0.1:42", 42));
        assert!(check_origin("vscode-webview://it", "127.0.0.1:0", 42));
    }

    #[test]
    fn test_origin_manually_binding() {
        assert!(!check_origin("https://huh.io:8080", "huh.io:42", 42));
        assert!(!check_origin("http://huh.io:8080", "huh.io:42", 42));
        assert!(!check_origin("https://huh.io:443", "huh.io:42", 42));
        assert!(check_origin("http://huh.io:42", "huh.io:0", 42));
        assert!(!check_origin("http://huh.io", "huh.io:42", 42));
        assert!(!check_origin("https://huh.io", "huh.io:42", 42));

        assert!(check_origin("http://127.0.0.1:42", "huh.io:42", 42));
        assert!(check_origin("http://127.0.0.1:42", "huh.io:42", 42));
        assert!(check_origin("http://127.0.0.1:42", "huh.io:0", 42));
        assert!(check_origin("http://localhost:42", "huh.io:42", 42));
        assert!(check_origin("http://localhost:42", "huh.io:0", 42));

        assert!(!check_origin("https://huh2.io:8080", "huh.io:42", 42));
        assert!(!check_origin("http://huh2.io:8080", "huh.io:42", 42));
        assert!(!check_origin("https://huh2.io:443", "huh.io:42", 42));
        assert!(!check_origin("http://huh2.io:42", "huh.io:0", 42));
        assert!(!check_origin("http://huh2.io", "huh.io:42", 42));
        assert!(!check_origin("https://huh2.io", "huh.io:42", 42));
    }

    // https://github.com/Myriad-Dreamin/tinymist/issues/1350
    // the origin of code-server's proxy
    #[test]
    fn test_valid_origin_code_server_proxy() {
        assert!(check_origin(
            // The URL has path /proxy/45411 but that is not sent in the Origin header
            "http://localhost:8080",
            "127.0.0.1:42",
            42
        ));
        assert!(check_origin("http://localhost", "127.0.0.1:42", 42));
    }

    // the origin of gitpod
    #[test]
    fn test_valid_origin_gitpod_proxy() {
        fn check_gitpod_origin(
            origin: &'static str,
            static_file_addr: &str,
            port: u16,
            workspace: &str,
            cluster_host: &str,
        ) -> bool {
            is_valid_origin_impl(
                &HeaderValue::from_static(origin),
                static_file_addr,
                port,
                &Some((workspace.to_owned(), cluster_host.to_owned())),
            )
        }

        let check_gitpod_origin1 = |origin: &'static str| {
            let explicit =
                check_gitpod_origin(origin, "127.0.0.1:42", 42, "workspace_id", "gitpod.typ");
            let implicit =
                check_gitpod_origin(origin, "127.0.0.1:0", 42, "workspace_id", "gitpod.typ");

            assert_eq!(explicit, implicit, "failed port binding");
            explicit
        };

        assert!(check_gitpod_origin1("http://127.0.0.1:42"));
        assert!(check_gitpod_origin1("http://127.0.0.1:42"));
        assert!(check_gitpod_origin1("https://42-workspace_id.gitpod.typ"));
        assert!(!check_gitpod_origin1(
            // A path is not allowed in Origin header
            "https://42-workspace_id.gitpod.typ/path"
        ));
        assert!(!check_gitpod_origin1(
            // Gitpod always runs on default port
            "https://42-workspace_id.gitpod.typ:42"
        ));

        assert!(!check_gitpod_origin1("https://42-workspace_id2.gitpod.typ"));
        assert!(!check_gitpod_origin1("http://huh.io"));
        assert!(!check_gitpod_origin1("https://huh.io"));
    }
}
