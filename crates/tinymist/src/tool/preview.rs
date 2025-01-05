//! Document preview tool for Typst

mod auth;

use std::num::NonZeroUsize;
use std::{collections::HashMap, net::SocketAddr, path::Path, sync::Arc};

use futures::{SinkExt, StreamExt, TryStreamExt};
use hyper::service::service_fn;
use hyper_tungstenite::{tungstenite::Message, HyperWebsocketStream};
use hyper_util::rt::TokioIo;
use hyper_util::server::graceful::GracefulShutdown;
use lsp_types::notification::Notification;
use reflexo_typst::debug_loc::SourceSpanOffset;
use reflexo_typst::vfs::notify::{FileChangeSet, MemoryEvent};
use reflexo_typst::{error::prelude::*, EntryReader, Error, TypstDocument, TypstFileId};
use serde::Serialize;
use serde_json::Value as JsonValue;
use sync_lsp::just_ok;
use tinymist_assets::TYPST_PREVIEW_HTML;
use tokio::sync::{mpsc, oneshot};
use typst::layout::{Frame, FrameItem, Point, Position};
use typst::syntax::{LinkedNode, Source, Span, SyntaxKind, VirtualPath};
use typst::World;
pub use typst_preview::CompileStatus;
use typst_preview::{
    CompileHost, ControlPlaneMessage, ControlPlaneResponse, ControlPlaneRx, ControlPlaneTx,
    DocToSrcJumpInfo, EditorServer, Location, MemoryFiles, MemoryFilesShort, PreviewArgs,
    PreviewBuilder, PreviewMode, Previewer, SourceFileServer, WsMessage,
};
use typst_shim::syntax::LinkedNodeExt;

use crate::world::{LspCompilerFeat, LspWorld};
use crate::*;
use actor::preview::{PreviewActor, PreviewRequest, PreviewTab};
use actor::typ_client::CompileHandler;
use actor::typ_server::{CompileServerActor, CompileServerOpts, SucceededArtifact};

impl CompileHost for CompileHandler {}

impl CompileHandler {
    fn resolve_source_span(world: &LspWorld, loc: Location) -> Option<SourceSpanOffset> {
        let Location::Src(loc) = loc;

        let filepath = Path::new(&loc.filepath);
        let relative_path = filepath.strip_prefix(&world.workspace_root()?).ok()?;

        let source_id = TypstFileId::new(None, VirtualPath::new(relative_path));
        let source = world.source(source_id).ok()?;
        let cursor = source.line_column_to_byte(loc.pos.line, loc.pos.column)?;

        let node = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
        if node.kind() != SyntaxKind::Text {
            return None;
        }
        let span = node.span();
        // todo: unicode char
        let offset = cursor.saturating_sub(node.offset());

        Some(SourceSpanOffset { span, offset })
    }

    async fn resolve_document_position(
        snap: SucceededArtifact<LspCompilerFeat>,
        loc: Location,
    ) -> Vec<Position> {
        let Location::Src(src_loc) = loc;

        let path = Path::new(&src_loc.filepath).to_owned();
        let line = src_loc.pos.line;
        let column = src_loc.pos.column;

        let doc = snap.success_doc();
        let Some(doc) = doc.as_deref() else {
            return vec![];
        };
        let world = snap.world();
        let Some(root) = world.workspace_root() else {
            return vec![];
        };
        let Some(relative_path) = path.strip_prefix(root).ok() else {
            return vec![];
        };

        let source_id = TypstFileId::new(None, VirtualPath::new(relative_path));
        let Some(source) = world.source(source_id).ok() else {
            return vec![];
        };
        let Some(cursor) = source.line_column_to_byte(line, column) else {
            return vec![];
        };

        jump_from_cursor(doc, &source, cursor)
    }

    fn resolve_source_location(
        world: &LspWorld,
        span: Span,
        offset: Option<usize>,
    ) -> Option<DocToSrcJumpInfo> {
        let resolve_off =
            |src: &Source, off: usize| src.byte_to_line(off).zip(src.byte_to_column(off));

        let source = world.source(span.id()?).ok()?;
        let mut range = source.find(span)?.range();
        if let Some(off) = offset {
            if off < range.len() {
                range.start += off;
            }
        }
        let filepath = world.path_for_id(span.id()?).ok()?;
        Some(DocToSrcJumpInfo {
            filepath: filepath.to_string_lossy().to_string(),
            start: resolve_off(&source, range.start),
            end: resolve_off(&source, range.end),
        })
    }
}

impl SourceFileServer for CompileHandler {
    /// fixme: character is 0-based, UTF-16 code unit.
    /// We treat it as UTF-8 now.
    async fn resolve_source_span(&self, loc: Location) -> Result<Option<SourceSpanOffset>, Error> {
        let snap = self.snapshot()?.receive().await?;
        Ok(Self::resolve_source_span(&snap.world, loc))
    }

    /// fixme: character is 0-based, UTF-16 code unit.
    /// We treat it as UTF-8 now.
    async fn resolve_document_position(&self, loc: Location) -> Result<Vec<Position>, Error> {
        let snap = self.artifact()?.receive().await?;
        Ok(Self::resolve_document_position(snap, loc).await)
    }

    async fn resolve_source_location(
        &self,
        span: Span,
        offset: Option<usize>,
    ) -> Result<Option<DocToSrcJumpInfo>, Error> {
        let snap = self.snapshot()?.receive().await?;
        Ok(Self::resolve_source_location(&snap.world, span, offset))
    }
}

impl EditorServer for CompileHandler {
    async fn update_memory_files(
        &self,
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
        self.add_memory_changes(if reset_shadow {
            MemoryEvent::Sync(files)
        } else {
            MemoryEvent::Update(files)
        });

        Ok(())
    }

    async fn remove_shadow_files(&self, files: MemoryFilesShort) -> Result<(), Error> {
        // todo: is it safe to believe that the path is normalized?
        let files = FileChangeSet::new_removes(files.files.into_iter().map(From::from).collect());
        self.add_memory_changes(MemoryEvent::Update(files));

        Ok(())
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

    /// (File) Host for the preview server. Note: if it equals to
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

    /// Don't open the preview in the browser after compilation.
    #[clap(long = "no-open")]
    pub dont_open_in_browser: bool,

    /// Use this to disable websocket authentication for the control plane server. Careful: Among other things, this allows any website you visit to use the control plane server.
    ///
    /// This option is only meant to ease the transition to authentication for downstream packages. It will be removed in a future version of tinymist.
    #[clap(long, default_value = "false")]
    pub disable_control_plane_auth: bool,
}

/// The global state of the preview tool.
pub struct PreviewState {
    /// Connection to the LSP client.
    client: TypedLspClient<PreviewState>,
    /// The backend running actor.
    preview_tx: mpsc::UnboundedSender<PreviewRequest>,
}

impl PreviewState {
    /// Create a new preview state.
    pub fn new(client: TypedLspClient<PreviewState>) -> Self {
        let (preview_tx, preview_rx) = mpsc::unbounded_channel();

        client.handle.spawn(
            PreviewActor {
                client: client.clone().to_untyped(),
                tabs: HashMap::default(),
                preview_rx,
            }
            .run(),
        );

        Self { client, preview_tx }
    }
}

/// Response for starting a preview.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPreviewResponse {
    static_server_port: Option<u16>,
    static_server_addr: Option<String>,
    data_plane_port: Option<u16>,
    secret: String,
    is_primary: bool,
}

impl PreviewState {
    /// Start a preview on a given compiler.
    pub fn start(
        &self,
        args: PreviewCliArgs,
        previewer: PreviewBuilder,
        compile_handler: Arc<CompileHandler>,
        is_primary: bool,
    ) -> SchedulableResponse<StartPreviewResponse> {
        let task_id = args.preview.task_id.clone();
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
                    EditorScrollTo(s) => client.send_notification::<ScrollSource>(s),
                    Outline(s) => client.send_notification::<NotifDocumentOutline>(s),
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

            let secret = auth::generate_token();

            let srv = make_http_server(
                true,
                args.data_plane_host,
                Some(secret.clone()),
                websocket_tx,
            )
            .await;
            let addr = srv.addr;
            log::info!("PreviewTask({task_id}): preview server listening on: {addr}");

            let resp = StartPreviewResponse {
                static_server_port: Some(addr.port()),
                static_server_addr: Some(addr.to_string()),
                data_plane_port: Some(addr.port()),
                secret,
                is_primary,
            };

            let sent = preview_tx.send(PreviewRequest::Started(PreviewTab {
                task_id,
                previewer,
                srv,
                ctl_tx,
                compile_handler,
                is_primary,
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
    serve_frontend_html: bool,
    static_file_addr: String,
    secret: Option<String>,
    websocket_tx: mpsc::UnboundedSender<HyperWebsocketStream>,
) -> HttpServer {
    use http_body_util::Full;
    use hyper::body::{Bytes, Incoming};
    type Server = hyper_util::server::conn::auto::Builder<hyper_util::rt::TokioExecutor>;

    let make_service = move || {
        let websocket_tx = websocket_tx.clone();
        let secret = secret.clone();
        service_fn(move |mut req: hyper::Request<Incoming>| {
            let websocket_tx = websocket_tx.clone();
            let secret = secret.clone();
            async move {
                // Check if the request is a websocket upgrade request.
                if hyper_tungstenite::is_upgrade_request(&req) {
                    let (response, websocket) = hyper_tungstenite::upgrade(&mut req, None)
                        .map_err(|e| {
                            log::error!("Error in websocket upgrade: {e}");
                            // let e = Error::new(e);
                        })
                        .unwrap();

                    tokio::spawn(async move {
                        let websocket = websocket.await.unwrap();

                        // Authenticate the client before we talk to it.
                        // Important even if we run on localhost because
                        // 1) browsers allow any website to connect to http servers/websockets on localhost
                        // 2) on multi-user systems another (potentially untrusted) user can connect to localhost.
                        //
                        // Note: We use authentication only for the websocket. The static HTML file server (see below)
                        //       only serves a not secret static template, so we don't bother with authentication there.
                        match secret {
                            Some(secret) => {
                                if let Ok(websocket) =
                                    auth::try_auth_websocket_client(websocket, &secret).await
                                {
                                    let _ = websocket_tx.send(websocket);
                                } else {
                                    log::error!("Websocket client authentication failed");
                                }
                            }
                            None => {
                                // We optionally allow to skip authentication upon explicit request to ease the transition to
                                // authentication for downstream packages.
                                // FIXME: Remove this is in a future version.
                                let _ = websocket_tx.send(websocket);
                            }
                        }
                    });

                    // Return the response so the spawned future can continue.
                    Ok(response)
                } else if req.uri().path() == "/" {
                    // log::debug!("Serve frontend: {mode:?}");
                    let res = hyper::Response::builder()
                        .header(hyper::header::CONTENT_TYPE, "text/html")
                        // It's important that we serve a static template that only contains information that is public anyway.
                        // Otherwise, we need authentication here (see comment for websocket case above).
                        // In particular, the websocket port, the secret etc. must not be in the HTML we serve. These information
                        // are in the # part of the URL.
                        .body(Full::<Bytes>::from(if serve_frontend_html {
                            TYPST_PREVIEW_HTML
                        } else {
                            ""
                        }))
                        .unwrap();
                    Ok::<_, anyhow::Error>(res)
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

    let listener = tokio::net::TcpListener::bind(&static_file_addr)
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    log::info!("preview server listening on http://{addr}");

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
            if let Err(err) = conn.await {
                log::error!("Error serving connection: {err:?}");
            }
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

/// Entry point of the preview tool.
pub async fn preview_main(args: PreviewCliArgs) -> anyhow::Result<()> {
    log::info!("Arguments: {args:#?}");

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

    let (service, handle) = {
        // type EditorSender = mpsc::UnboundedSender<EditorRequest>;
        let (editor_tx, mut editor_rx) = mpsc::unbounded_channel();
        let (intr_tx, intr_rx) = mpsc::unbounded_channel();

        let handle = Arc::new(CompileHandler {
            inner: Default::default(),
            diag_group: "main".to_owned(),
            intr_tx: intr_tx.clone(),
            // export_tx,
            export: Default::default(),
            editor_tx,
            analysis: Arc::default(),
            stats: Default::default(),
            notified_revision: parking_lot::Mutex::new(0),
        });

        // Consume editor_rx
        tokio::spawn(async move { while editor_rx.recv().await.is_some() {} });

        let service = CompileServerActor::new_with(
            verse,
            intr_tx,
            intr_rx,
            CompileServerOpts {
                compile_handle: handle.clone(),
                ..Default::default()
            },
        )
        .with_watch(true);

        (service, handle)
    };

    let secret = auth::generate_token();
    log::info!("Secret for websocket authentication: {secret}");

    let (lsp_tx, mut lsp_rx) = ControlPlaneTx::new(true);

    let secret_for_control_plane = if args.disable_control_plane_auth {
        log::warn!(
            "Disabling authentication for the control plane server. This is not recommended."
        );
        None
    } else {
        Some(secret.clone())
    };
    let control_plane_server_handle = tokio::spawn(async move {
        let (control_sock_tx, mut control_sock_rx) = mpsc::unbounded_channel();

        let srv = make_http_server(
            false,
            args.control_plane_host,
            secret_for_control_plane,
            control_sock_tx,
        )
        .await;
        log::info!("Control panel server listening on: {}", srv.addr);

        let control_websocket = control_sock_rx.recv().await.unwrap();
        let ws = control_websocket;

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

    let previewer = PreviewBuilder::new(args.preview);
    let registered = handle.register_preview(previewer.compile_watcher());
    assert!(registered, "failed to register preview");
    let (websocket_tx, websocket_rx) = mpsc::unbounded_channel();
    let mut previewer = previewer.build(lsp_tx, handle.clone()).await;
    tokio::spawn(service.run());

    bind_streams(&mut previewer, websocket_rx);

    let static_server = if let Some(static_file_host) = static_file_host {
        log::warn!("--static-file-host is deprecated, which will be removed in the future. Use --data-plane-host instead.");
        Some(
            make_http_server(
                true,
                static_file_host,
                Some(secret.clone()),
                websocket_tx.clone(),
            )
            .await,
        )
    } else {
        None
    };

    let srv = make_http_server(
        true,
        args.data_plane_host,
        Some(secret.clone()),
        websocket_tx,
    )
    .await;
    log::info!("Data plane server listening on: {}", srv.addr);

    let static_server_addr = static_server.as_ref().map(|s| s.addr).unwrap_or(srv.addr);
    let preview_url = format!(
        "http://{static_server_addr}/#secret={secret}&previewMode={}",
        match args.preview_mode {
            PreviewMode::Document => "Doc",
            PreviewMode::Slide => "Slide",
        }
    );
    log::info!("Static file server listening on: {preview_url}");

    if !args.dont_open_in_browser {
        if let Err(e) = open::that_detached(preview_url) {
            log::error!("failed to open browser: {e}");
        };
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
    let mut positions: Vec<Position> = vec![];
    for (i, page) in document.pages.iter().enumerate() {
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
                    let dis = glyph.span.0.number().abs_diff(span.number());
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

fn bind_streams(
    previewer: &mut Previewer,
    websocket_rx: mpsc::UnboundedReceiver<HyperWebsocketStream>,
) {
    previewer.start_data_plane(websocket_rx, |conn: HyperWebsocketStream| {
        conn.sink_map_err(|e| error_once!("cannot serve_with websocket", err: e.to_string()))
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
            })
    });
}
