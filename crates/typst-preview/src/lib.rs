mod actor;
mod args;
mod debug_loc;
mod outline;

pub use actor::editor::{
    CompileStatus, ControlPlaneMessage, ControlPlaneResponse, LspControlPlaneRx, LspControlPlaneTx,
};
use actor::webview::WebviewActorRequest;
pub use args::*;
use once_cell::sync::OnceCell;
pub use outline::Outline;

use std::pin::Pin;
use std::time::Duration;
use std::{collections::HashMap, future::Future, path::PathBuf, sync::Arc};

use debug_loc::SpanInterner;
use futures::SinkExt;
use log::info;
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use typst::{layout::Position, syntax::Span};
use typst_ts_core::debug_loc::SourceSpanOffset;
use typst_ts_core::Error;
use typst_ts_core::{ImmutStr, TypstDocument as Document};

use crate::actor::editor::EditorActorRequest;
use crate::actor::render::RenderActorRequest;
use actor::editor::{EditorActor, EditorConnection};
use actor::typst::{TypstActor, TypstActorRequest};

type StopFuture = Pin<Box<dyn Future<Output = ()> + Send + Sync>>;

pub struct Previewer {
    frontend_html_factory: Box<dyn Fn(PreviewMode) -> ImmutStr + Send + Sync>,
    stop: Option<Box<dyn FnOnce() -> StopFuture + Send + Sync>>,
    data_plane_handle: tokio::task::JoinHandle<()>,
    control_plane_handle: tokio::task::JoinHandle<()>,
    data_plane_port: u16,
}

impl Previewer {
    /// Get the HTML for the frontend by a given preview mode
    pub fn frontend_html(&self, mode: PreviewMode) -> ImmutStr {
        (self.frontend_html_factory)(mode)
    }

    pub fn data_plane_port(&self) -> u16 {
        self.data_plane_port
    }

    /// Join the previewer actors.
    pub async fn join(self) {
        let _ = tokio::join!(self.data_plane_handle, self.control_plane_handle);
    }

    pub async fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop().await;
        }
    }
}

pub trait CompileHost: SourceFileServer + EditorServer {}

pub async fn preview<T: CompileHost + Send + Sync + 'static>(
    arguments: PreviewArgs,
    client: Arc<T>,
    html: &str,
) -> Previewer {
    PreviewBuilder::new(arguments).start(client, html).await
}

async fn preview_<T: CompileHost + Send + Sync + 'static>(
    builder: PreviewBuilder,
    client: Arc<T>,
    html: &str,
) -> Previewer {
    let PreviewBuilder {
        arguments,
        lsp_connection,
        typst_mailbox,
        renderer_mailbox,
        editor_conn,
        webview_conn: (webview_tx, _),
        doc_sender,
        ..
    } = builder;
    let enable_partial_rendering = arguments.enable_partial_rendering;
    let invert_colors = arguments.invert_colors;
    let idle_timeout = Duration::from_secs(5);

    // Shared resource
    let span_interner = SpanInterner::new();

    // Spawns the typst actor
    let typst_actor = TypstActor::new(
        client,
        typst_mailbox.1,
        renderer_mailbox.0.clone(),
        editor_conn.0.clone(),
        webview_tx.clone(),
    );
    tokio::spawn(typst_actor.run());

    log::info!("Previewer: typst actor spawned");

    let (data_plane_port_tx, data_plane_port_rx) = oneshot::channel();
    let data_plane_addr = arguments.data_plane_host;
    let (shutdown_data_plane_tx, mut shutdown_data_plane_rx) = mpsc::channel(1);
    let data_plane_handle = {
        let span_interner = span_interner.clone();
        let typst_tx = typst_mailbox.0.clone();
        let webview_tx = webview_tx.clone();
        let renderer_tx = renderer_mailbox.0.clone();
        let editor_tx = editor_conn.0.clone();
        let shutdown_tx = lsp_connection.as_ref().map(|e| e.shutdown_tx.clone());
        tokio::spawn(async move {
            // Create the event loop and TCP listener we'll accept connections on.
            let try_socket = TcpListener::bind(&data_plane_addr).await;
            let listener = try_socket.expect("Failed to bind");
            info!(
                "Data plane server listening on: {}",
                listener.local_addr().unwrap()
            );
            let _ = data_plane_port_tx.send(listener.local_addr().unwrap().port());
            let (alive_tx, mut alive_rx) = mpsc::unbounded_channel();
            let recv = |stream: TcpStream| async {
                let span_interner = span_interner.clone();
                let webview_tx = webview_tx.clone();
                let webview_rx = webview_tx.subscribe();
                let typst_tx = typst_tx.clone();
                let mut conn = accept_connection(stream).await;
                if enable_partial_rendering {
                    conn.send(Message::Binary("partial-rendering,true".into()))
                        .await
                        .unwrap();
                }
                if !invert_colors.is_empty() {
                    conn.send(Message::Binary(
                        format!("invert-colors,{}", invert_colors).into(),
                    ))
                    .await
                    .unwrap();
                }
                let actor::webview::Channels { svg } =
                    actor::webview::WebviewActor::set_up_channels();
                let webview_actor = actor::webview::WebviewActor::new(
                    conn,
                    svg.1,
                    webview_tx.clone(),
                    webview_rx,
                    editor_tx.clone(),
                    renderer_tx.clone(),
                );

                let alive_tx = alive_tx.clone();
                tokio::spawn(async move {
                    struct FinallySend(mpsc::UnboundedSender<()>);
                    impl Drop for FinallySend {
                        fn drop(&mut self) {
                            let _ = self.0.send(());
                        }
                    }

                    let _send = FinallySend(alive_tx);
                    webview_actor.run().await;
                });
                let render_actor = actor::render::RenderActor::new(
                    renderer_tx.subscribe(),
                    doc_sender.clone(),
                    typst_tx,
                    svg.0,
                    webview_tx,
                );
                tokio::spawn(render_actor.run());
                let outline_render_actor = actor::render::OutlineRenderActor::new(
                    renderer_tx.subscribe(),
                    doc_sender.clone(),
                    editor_tx.clone(),
                    span_interner,
                );
                tokio::spawn(outline_render_actor.run());
            };

            let mut alive_cnt = 0;
            let mut shutdown_bell = tokio::time::interval(idle_timeout);
            loop {
                if shutdown_tx.is_some() {
                    shutdown_bell.reset();
                }
                tokio::select! {
                    Some(()) = shutdown_data_plane_rx.recv() => {
                        info!("Data plane server shutdown");
                        return;
                    }
                    Ok((stream, _)) = listener.accept() => {
                        alive_cnt += 1;
                        recv(stream).await;
                    },
                    _ = alive_rx.recv() => {
                        alive_cnt -= 1;
                    }
                    _ = shutdown_bell.tick(), if alive_cnt == 0 && shutdown_tx.is_some() => {
                        let shutdown_tx = shutdown_tx.expect("scheduled shutdown without shutdown_tx");

                        info!("Data plane server has been idle for {idle_timeout:?}, shutting down.");
                        let _ = shutdown_tx.send(()).await;
                        info!("Data plane server shutdown");
                        return;
                    }
                }
            }
        })
    };

    let control_plane_addr = arguments.control_plane_host;
    let control_plane_handle = {
        let span_interner = span_interner.clone();
        let typst_tx = typst_mailbox.0.clone();
        let editor_rx = editor_conn.1;
        tokio::spawn(async move {
            let conn = if !control_plane_addr.is_empty() {
                let try_socket = TcpListener::bind(&control_plane_addr).await;
                let listener = try_socket.expect("Failed to bind");
                info!(
                    "Control plane server listening on: {}",
                    listener.local_addr().unwrap()
                );
                let (stream, _) = listener.accept().await.unwrap();

                let conn = accept_connection(stream).await;

                EditorConnection::WebSocket(conn)
            } else {
                EditorConnection::Lsp(lsp_connection.unwrap())
            };

            let editor_actor =
                EditorActor::new(editor_rx, conn, typst_tx, webview_tx, span_interner);
            editor_actor.run().await;
            info!("Control plane client shutdown");
        })
    };
    let data_plane_port = data_plane_port_rx.await.unwrap();
    let html = html.replace(
        "ws://127.0.0.1:23625",
        format!("ws://127.0.0.1:{data_plane_port}").as_str(),
    );
    // previewMode
    let frontend_html_factory = Box::new(move |mode| -> ImmutStr {
        let mode = match mode {
            PreviewMode::Document => "Doc",
            PreviewMode::Slide => "Slide",
        };
        html.replace(
            "preview-arg:previewMode:Doc",
            format!("preview-arg:previewMode:{}", mode).as_str(),
        )
        .into()
    });

    let editor_tx = editor_conn.0;
    let stop = move || -> StopFuture {
        Box::pin(async move {
            let _ = shutdown_data_plane_tx.send(()).await;
            let _ = editor_tx.send(EditorActorRequest::Shutdown);
        })
    };

    Previewer {
        frontend_html_factory,
        data_plane_handle,
        control_plane_handle,
        data_plane_port,
        stop: Some(Box::new(stop)),
    }
}

type MpScChannel<T> = (mpsc::UnboundedSender<T>, mpsc::UnboundedReceiver<T>);
type BroadcastChannel<T> = (broadcast::Sender<T>, broadcast::Receiver<T>);

pub struct PreviewBuilder {
    arguments: PreviewArgs,
    lsp_connection: Option<LspControlPlaneTx>,

    typst_mailbox: MpScChannel<TypstActorRequest>,
    renderer_mailbox: BroadcastChannel<RenderActorRequest>,
    editor_conn: MpScChannel<EditorActorRequest>,
    webview_conn: BroadcastChannel<WebviewActorRequest>,
    doc_sender: Arc<std::sync::RwLock<Option<Arc<Document>>>>,

    compile_watcher: OnceCell<Arc<CompileWatcher>>,
}

impl PreviewBuilder {
    pub fn new(arguments: PreviewArgs) -> Self {
        Self {
            arguments,
            lsp_connection: None,
            typst_mailbox: mpsc::unbounded_channel(),
            renderer_mailbox: broadcast::channel(1024),
            editor_conn: mpsc::unbounded_channel(),
            webview_conn: broadcast::channel(32),
            doc_sender: Arc::new(std::sync::RwLock::new(None)),
            compile_watcher: OnceCell::new(),
        }
    }

    pub fn with_lsp_connection(mut self, lsp_connection: Option<LspControlPlaneTx>) -> Self {
        self.lsp_connection = lsp_connection;
        self
    }

    pub fn compile_watcher(&self) -> &Arc<CompileWatcher> {
        self.compile_watcher.get_or_init(|| {
            Arc::new(CompileWatcher {
                task_id: self.arguments.task_id.clone(),
                refresh_style: self.arguments.refresh_style,
                doc_sender: self.doc_sender.clone(),
                editor_tx: self.editor_conn.0.clone(),
                render_tx: self.renderer_mailbox.0.clone(),
            })
        })
    }

    pub async fn start<T>(self, client: Arc<T>, html: &str) -> Previewer
    where
        T: CompileHost + Send + Sync + 'static,
    {
        preview_(self, client, html).await
    }
}

pub type SourceLocation = typst_ts_core::debug_loc::SourceLocation;

pub enum Location {
    Src(SourceLocation),
}

pub trait SourceFileServer {
    fn resolve_source_span(
        &self,
        _by: Location,
    ) -> impl Future<Output = Result<Option<SourceSpanOffset>, Error>> + Send {
        async { Ok(None) }
    }

    fn resolve_document_position(
        &self,
        _by: Location,
    ) -> impl Future<Output = Result<Option<Position>, Error>> + Send {
        async { Ok(None) }
    }

    fn resolve_source_location(
        &self,
        _s: Span,
        _offset: Option<usize>,
    ) -> impl Future<Output = Result<Option<DocToSrcJumpInfo>, Error>> + Send {
        async { Ok(None) }
    }
}

pub trait EditorServer {
    fn update_memory_files(
        &self,
        _files: MemoryFiles,
        _reset_shadow: bool,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async { Ok(()) }
    }

    fn remove_shadow_files(
        &self,
        _files: MemoryFilesShort,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async { Ok(()) }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DocToSrcJumpInfo {
    pub filepath: String,
    pub start: Option<(usize, usize)>, // row, column
    pub end: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChangeCursorPositionRequest {
    filepath: PathBuf,
    line: usize,
    /// fixme: character is 0-based, UTF-16 code unit.
    /// We treat it as UTF-8 now.
    character: usize,
}

#[derive(Debug, Deserialize)]
pub struct SrcToDocJumpRequest {
    filepath: PathBuf,
    line: usize,
    /// fixme: character is 0-based, UTF-16 code unit.
    /// We treat it as UTF-8 now.
    character: usize,
}

impl SrcToDocJumpRequest {
    pub fn to_byte_offset(&self, src: &typst::syntax::Source) -> Option<usize> {
        src.line_column_to_byte(self.line, self.character)
    }
}

#[derive(Debug, Deserialize)]
pub struct MemoryFiles {
    pub files: HashMap<PathBuf, String>,
}

#[derive(Debug, Deserialize)]
pub struct MemoryFilesShort {
    pub files: Vec<PathBuf>,
    // mtime: Option<u64>,
}

pub struct CompileWatcher {
    task_id: String,
    refresh_style: RefreshStyle,
    doc_sender: Arc<std::sync::RwLock<Option<Arc<Document>>>>,
    editor_tx: mpsc::UnboundedSender<EditorActorRequest>,
    render_tx: broadcast::Sender<RenderActorRequest>,
}

impl CompileWatcher {
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    pub fn status(&self, status: CompileStatus) {
        let _ = self
            .editor_tx
            .send(EditorActorRequest::CompileStatus(status));
    }

    pub fn notify_compile(
        &self,
        res: Result<Arc<Document>, CompileStatus>,
        is_on_saved: bool,
        is_by_entry_update: bool,
    ) {
        if !is_by_entry_update && (self.refresh_style == RefreshStyle::OnSave && !is_on_saved) {
            return;
        }

        match res {
            Ok(doc) => {
                // it is ok to ignore the error here
                *self.doc_sender.write().unwrap() = Some(doc);

                // todo: is it right that ignore zero broadcast receiver?
                let _ = self.render_tx.send(RenderActorRequest::RenderIncremental);
                let _ = self.editor_tx.send(EditorActorRequest::CompileStatus(
                    CompileStatus::CompileSuccess,
                ));
            }
            Err(status) => {
                let _ = self
                    .editor_tx
                    .send(EditorActorRequest::CompileStatus(status));
            }
        }
    }
}

async fn accept_connection(stream: TcpStream) -> WebSocketStream<TcpStream> {
    let addr = stream
        .peer_addr()
        .expect("connected streams should have a peer address");
    info!("Peer address: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(stream)
        .await
        .expect("Error during the websocket handshake occurred");

    info!("New WebSocket connection: {}", addr);
    ws_stream
}
