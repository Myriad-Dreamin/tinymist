//! Document preview tool for Typst

use std::convert::Infallible;
use std::future::Future;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::time::Duration;
use std::{collections::HashMap, net::SocketAddr, path::Path, sync::Arc};

use actor::typ_server::{CompileServerOpts, SucceededArtifact};
use futures::{SinkExt, TryStreamExt};
use hyper::service::service_fn;
use hyper_tungstenite::HyperWebsocketStream;
use hyper_util::rt::TokioIo;
use lsp_types::notification::Notification;
use reflexo::error::prelude::*;
use reflexo::error_once;
use reflexo_typst::debug_loc::SourceSpanOffset;
use reflexo_typst::vfs::notify::{FileChangeSet, MemoryEvent};
use reflexo_typst::{EntryReader, Error, TypstDocument, TypstFileId};
use serde::Serialize;
use serde_json::Value as JsonValue;
use sync_lsp::just_ok;
use tinymist_assets::TYPST_PREVIEW_HTML;
use tinymist_query::analysis::Analysis;
use tokio::sync::{mpsc, oneshot};
use typst::layout::{Frame, FrameItem, Point, Position};
use typst::syntax::{LinkedNode, Source, Span, SyntaxKind, VirtualPath};
use typst::World;
pub use typst_preview::CompileStatus;
use typst_preview::{
    CompileHost, ControlPlaneMessage, ControlPlaneResponse, DocToSrcJumpInfo, EditorConnection,
    EditorServer, Location, LspControlPlaneRx, LspControlPlaneTx, MemoryFiles, MemoryFilesShort,
    PreviewArgs, PreviewBuilder, PreviewMode, Previewer, SourceFileServer,
};

use crate::world::{LspCompilerFeat, LspWorld};
use crate::*;
use actor::{
    preview::{PreviewActor, PreviewRequest, PreviewTab},
    typ_client::CompileHandler,
    typ_server::CompileServerActor,
};

type ErrConverter = fn(hyper_tungstenite::tungstenite::Error) -> reflexo_typst::Error;
type WsConn = futures::sink::SinkMapErr<
    futures::stream::MapErr<HyperWebsocketStream, ErrConverter>,
    ErrConverter,
>;
type ToWsConn = Pin<Box<dyn Future<Output = WsConn> + Send>>;

impl CompileHost for CompileHandler {}

impl CompileHandler {
    fn resolve_source_span(world: &LspWorld, loc: Location) -> Option<SourceSpanOffset> {
        let Location::Src(loc) = loc;

        let filepath = Path::new(&loc.filepath);
        let relative_path = filepath.strip_prefix(&world.workspace_root()?).ok()?;

        let source_id = TypstFileId::new(None, VirtualPath::new(relative_path));
        let source = world.source(source_id).ok()?;
        let cursor = source.line_column_to_byte(loc.pos.line, loc.pos.column)?;

        let node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        if node.kind() != SyntaxKind::Text {
            return None;
        }
        let span = node.span();
        // todo: unicode char
        let offset = cursor.saturating_sub(node.offset());

        Some(SourceSpanOffset { span, offset })
    }

    async fn resolve_document_position(
        snap: &SucceededArtifact<LspCompilerFeat>,
        loc: Location,
    ) -> Option<Position> {
        let Location::Src(src_loc) = loc;

        let path = Path::new(&src_loc.filepath).to_owned();
        let line = src_loc.pos.line;
        let column = src_loc.pos.column;

        let doc = snap.success_doc();
        let doc = doc.as_deref()?;
        let world = snap.world();

        let relative_path = path.strip_prefix(&world.workspace_root()?).ok()?;

        let source_id = TypstFileId::new(None, VirtualPath::new(relative_path));
        let source = world.source(source_id).ok()?;
        let cursor = source.line_column_to_byte(line, column)?;

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
    async fn resolve_document_position(&self, loc: Location) -> Result<Option<Position>, Error> {
        let snap = self.artifact()?.receive().await?;
        Ok(Self::resolve_document_position(&snap, loc).await)
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

    /// Data plane server will bind to this address
    #[clap(
        long = "data-plane-host",
        default_value = "127.0.0.1:23625",
        value_name = "HOST",
        hide(true)
    )]
    pub data_plane_host: String,

    /// Control plane server will bind to this address
    #[cfg_attr(
        feature = "clap",
        clap(
            long = "control-plane-host",
            default_value = "127.0.0.1:23626",
            value_name = "HOST",
            hide(true)
        )
    )]
    pub control_plane_host: String,

    /// (File) Host for the preview server
    #[clap(
        long = "host",
        value_name = "HOST",
        default_value = "127.0.0.1:23627",
        alias = "static-file-host"
    )]
    pub static_file_host: String,

    /// Let it not be the primary instance.
    #[clap(long = "not-primary", hide(true))]
    pub not_as_primary: bool,

    /// Don't open the preview in the browser after compilation.
    #[clap(long = "no-open")]
    pub dont_open_in_browser: bool,
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
    is_primary: bool,
}

impl PreviewState {
    /// Start a preview on a given compiler.
    pub fn start(
        &self,
        args: PreviewCliArgs,
        mut previewer: PreviewBuilder,
        compile_handler: Arc<CompileHandler>,
        is_primary: bool,
    ) -> SchedulableResponse<StartPreviewResponse> {
        let task_id = args.preview.task_id.clone();
        log::info!("PreviewTask({task_id}): arguments: {args:#?}");

        let (lsp_tx, lsp_rx) = LspControlPlaneTx::new();
        let LspControlPlaneRx {
            resp_rx,
            ctl_tx,
            mut shutdown_rx,
        } = lsp_rx;

        // Create a previewer
        // previewer = previewer.with_lsp_connection(Some(lsp_tx));

        //
        // conn: EditorConnection<'static, C>,
        let conn = EditorConnection::Lsp(lsp_tx);

        let (websocket_tx, websocket_rx) = mpsc::unbounded_channel::<ToWsConn>();

        // websocket_rx: mpsc::UnboundedReceiver<HyperWebsocket>,
        // Create the event loop and TCP listener we'll accept connections on.
        // let try_socket = TcpListener::bind(&data_plane_addr).await;
        // let listener = try_socket.expect("Failed to bind");
        // info!(
        //     "Data plane server listening on: {}",
        //     listener.local_addr().unwrap()
        // );
        // let _ = data_plane_port_tx.send(listener.local_addr().unwrap().port());
        // let (alive_tx, mut alive_rx) = mpsc::unbounded_channel();

        // let mut http = hyper::server::conn::http1::Builder::new();
        // http.keep_alive(true);

        // let (data_plane_port_tx, data_plane_port_rx) = oneshot::channel();
        // let data_plane_addr = arguments.data_plane_host;
        // let data_plane_port = data_plane_port_rx.await.unwrap();
        // let url = format!("ws://127.0.0.1:{data_plane_port}");
        // let new_url = if gitpod::is_gitpod() {
        //     gitpod::translate_gitpod_url(&url).unwrap()
        // } else {
        //     url
        // };
        // C: futures::Sink<Message, Error = reflexo_typst::Error>
        // + futures::Stream<Item = Result<Message, reflexo_typst::Error>>
        // + Send
        // + Sync
        // + 'static,

        let data_plane_port = 0;

        let previewer = previewer.start(
            websocket_rx,
            conn,
            compile_handler.clone(),
            TYPST_PREVIEW_HTML,
        );

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
            let previewer = previewer.await;

            // Put a fence to ensure the previewer can receive the first compilation.
            // The fence must be put after the previewer is initialized.
            compile_handler.flush_compile();

            // Spawn a task to handle the websocket connection.
            // tokio::spawn(async move {
            //     if let Err(e) = serve_websocket(websocket).await {
            //         eprintln!("Error in websocket connection: {e}");
            //     }
            // });

            let (ss_addr, ss_killer, ss_handle) = make_static_host(
                &previewer,
                args.static_file_host,
                args.preview_mode,
                websocket_tx,
            )
            .await;
            log::info!("PreviewTask({task_id}): static file server listening on: {ss_addr}");

            let resp = StartPreviewResponse {
                static_server_port: Some(ss_addr.port()),
                static_server_addr: Some(ss_addr.to_string()),
                data_plane_port: Some(data_plane_port),
                is_primary,
            };

            let sent = preview_tx.send(PreviewRequest::Started(PreviewTab {
                task_id,
                previewer,
                ss_killer,
                ss_handle,
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

/// Create a static file server for the previewer.
pub async fn make_static_host(
    previewer: &Previewer,
    static_file_addr: String,
    mode: PreviewMode,
    websocket_tx: mpsc::UnboundedSender<ToWsConn>,
) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    use http_body_util::Full;
    use hyper::body::{Bytes, Incoming};

    let frontend_html = hyper::body::Bytes::from(previewer.frontend_html(mode));
    let make_service = move || {
        let frontend_html = frontend_html.clone();
        let websocket_tx = websocket_tx.clone();
        service_fn(move |mut req: hyper::Request<Incoming>| {
            let frontend_html = frontend_html.clone();
            let websocket_tx = websocket_tx.clone();
            async move {
                // Check if the request is a websocket upgrade request.
                if hyper_tungstenite::is_upgrade_request(&req) {
                    let (response, websocket) = hyper_tungstenite::upgrade(&mut req, None)
                        .map_err(|e| {
                            log::error!("Error in websocket upgrade: {e}");
                            // let e = Error::new(e);
                        })
                        .unwrap();
                    // hyper_tungstenite::HyperWebsocket

                    const CONVERT_ERR: fn(
                        hyper_tungstenite::tungstenite::Error,
                    ) -> reflexo_typst::Error = convert_err;

                    // tokio::spawn(async move {
                    // });
                    let _ = websocket_tx.send(Box::pin(async move {
                        let conn = websocket.await.unwrap();
                        conn.map_err(CONVERT_ERR).sink_map_err(CONVERT_ERR)
                    }));

                    // Return the response so the spawned future can continue.
                    Ok(response)
                } else if req.uri().path() == "/" {
                    log::debug!("Serve frontend: {mode:?}");
                    let res = hyper::Response::builder()
                        .header(hyper::header::CONTENT_TYPE, "text/html")
                        .body(Full::<Bytes>::from(frontend_html))
                        .unwrap();
                    Ok::<_, Infallible>(res)
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
    println!("Listening on http://{addr}");

    let (tx, rx) = tokio::sync::oneshot::channel();
    let (final_tx, final_rx) = tokio::sync::oneshot::channel();

    // the graceful watcher
    let graceful = hyper_util::server::graceful::GracefulShutdown::new();

    let join_handle = tokio::spawn(async move {
        // when this signal completes, start shutdown
        let mut signal = std::pin::pin!(final_rx);

        let mut server =
            hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new());
        server.http1().keep_alive(true);
        let server = server;

        loop {
            tokio::select! {
                conn = listener.accept() => {
                    let (stream, _peer_addr) = match conn {
                        Ok(conn) => conn,
                        Err(e) => {
                            eprintln!("accept error: {e}");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    };

                    let conn =
                        server.serve_connection_with_upgrades(TokioIo::new(Box::pin(stream)), make_service()).into_owned();

                        // watch this conn
                    let conn = graceful.watch(conn);
                    tokio::spawn(async move {

                        if let Err(err) = conn.await {
                            println!("Error serving connection: {err:?}");
                        }
                    });
                },
                Ok(_) = &mut signal => {
                    eprintln!("graceful shutdown signal received");
                    // stop the accept loop
                    break;
                }
            }
        }

        tokio::select! {
            _ = graceful.shutdown() => {
                eprintln!("Gracefully shutdown!");
            },
            _ = tokio::time::sleep(Duration::from_secs(10)) => {
                eprintln!("Waited 10 seconds for graceful shutdown, aborting...");
            }
        }
    });
    tokio::spawn(async move {
        let _ = rx.await;
        final_tx.send(()).ok();
        log::info!("Static file server joined");
    });
    (addr, tx, join_handle)
}

/// Entry point of the preview tool.
pub async fn preview_main(args: PreviewCliArgs) -> anyhow::Result<()> {
    log::info!("Arguments: {args:#?}");

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
            analysis: Analysis::default(),
            periscope: tinymist_render::PeriscopeRenderer::default(),
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

    // let control_plane_addr = arguments.control_plane_host;
    // let conn = if !control_plane_addr.is_empty() {
    //     let try_socket = TcpListener::bind(&control_plane_addr).await;
    //     let listener = try_socket.expect("Failed to bind");
    //     info!(
    //         "Control plane server listening on: {}",
    //         listener.local_addr().unwrap()
    //     );
    //     let (stream, _) = listener.accept().await.unwrap();

    //     let conn = accept_connection(stream).await;

    //     EditorConnection::WebSocket(conn)
    // } else {
    //     EditorConnection::Lsp(lsp_connection.unwrap())
    // };

    let conn = EditorConnection::WebSocket(todo!());

    let previewer = PreviewBuilder::new(args.preview);
    let registered = handle.register_preview(previewer.compile_watcher());
    assert!(registered, "failed to register preview");
    let (websocket_tx, websocket_rx) = mpsc::unbounded_channel::<ToWsConn>();
    let previewer = previewer
        .start(websocket_rx, conn, handle.clone(), TYPST_PREVIEW_HTML)
        .await;
    tokio::spawn(service.run());

    let (static_server_addr, _tx, static_server_handle) = make_static_host(
        &previewer,
        args.static_file_host,
        args.preview_mode,
        websocket_tx,
    )
    .await;
    log::info!("Static file server listening on: {}", static_server_addr);

    if !args.dont_open_in_browser {
        if let Err(e) = open::that_detached(format!("http://{static_server_addr}")) {
            log::error!("failed to open browser: {}", e);
        };
    }

    let _ = tokio::join!(previewer.join(), static_server_handle);

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
fn jump_from_cursor(document: &TypstDocument, source: &Source, cursor: usize) -> Option<Position> {
    let node = LinkedNode::new(source.root()).leaf_at(cursor)?;
    if node.kind() != SyntaxKind::Text {
        return None;
    }

    let mut min_dis = u64::MAX;
    let mut p = Point::default();
    let mut ppage = 0usize;

    let span = node.span();
    for (i, page) in document.pages.iter().enumerate() {
        let t_dis = min_dis;
        if let Some(pos) = find_in_frame(&page.frame, span, &mut min_dis, &mut p) {
            return Some(Position {
                page: NonZeroUsize::new(i + 1)?,
                point: pos,
            });
        }
        if t_dis != min_dis {
            ppage = i;
        }
    }

    if min_dis == u64::MAX {
        return None;
    }

    Some(Position {
        page: NonZeroUsize::new(ppage + 1)?,
        point: p,
    })
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

fn convert_err(e: hyper_tungstenite::tungstenite::Error) -> reflexo::Error {
    error_once!("cannot serve websocket", err: e.to_string())
}
