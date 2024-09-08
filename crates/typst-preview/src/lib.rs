mod actor;
mod args;
mod debug_loc;
mod outline;

pub use actor::editor::{
    CompileStatus, ControlPlaneMessage, ControlPlaneResponse, ControlPlaneRx, ControlPlaneTx,
};
pub use args::*;
pub use outline::Outline;

use std::{collections::HashMap, future::Future, path::PathBuf, pin::Pin, sync::Arc};

use futures::sink::SinkExt;
use once_cell::sync::OnceCell;
use reflexo_typst::debug_loc::SourceSpanOffset;
use reflexo_typst::Error;
use reflexo_typst::TypstDocument as Document;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use typst::{layout::Position, syntax::Span};

use actor::editor::{EditorActor, EditorActorRequest};
use actor::render::RenderActorRequest;
use actor::typst::{TypstActor, TypstActorRequest};
use actor::webview::WebviewActorRequest;
use debug_loc::SpanInterner;

type StopFuture = Pin<Box<dyn Future<Output = ()> + Send + Sync>>;

type WsError = Error;
type Message = WsMessage;

pub trait CompileHost: SourceFileServer + EditorServer {}

/// Get the HTML for the frontend by a given preview mode and server to connect
pub fn frontend_html(html: &str, mode: PreviewMode, to: &str) -> String {
    let mode = match mode {
        PreviewMode::Document => "Doc",
        PreviewMode::Slide => "Slide",
    };

    html.replace("ws://127.0.0.1:23625", to).replace(
        "preview-arg:previewMode:Doc",
        format!("preview-arg:previewMode:{mode}").as_str(),
    )
}

/// Shortcut to create a previewer.
pub async fn preview(
    arguments: PreviewArgs,
    conn: ControlPlaneTx,
    client: Arc<impl CompileHost + Send + Sync + 'static>,
) -> Previewer {
    PreviewBuilder::new(arguments).build(conn, client).await
}

pub struct Previewer {
    stop: Option<Box<dyn FnOnce() -> StopFuture + Send + Sync>>,
    data_plane_handle: Option<tokio::task::JoinHandle<()>>,
    data_plane_resources: Option<(DataPlane, Option<mpsc::Sender<()>>, mpsc::Receiver<()>)>,
    control_plane_handle: tokio::task::JoinHandle<()>,
}

impl Previewer {
    /// Send stop requests to preview actors.
    pub async fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop().await;
        }
    }

    /// Join all the previewer actors. Note: send stop request first.
    pub async fn join(mut self) {
        let data_plane_handle = self.data_plane_handle.take().expect("must bind data plane");
        let _ = tokio::join!(data_plane_handle, self.control_plane_handle);
    }

    /// Listen streams that accepting data plane messages.
    pub fn start_data_plane<
        C: futures::Sink<WsMessage, Error = WsError>
            + futures::Stream<Item = Result<WsMessage, WsError>>
            + Send
            + 'static,
        S: 'static,
        SFut: Future<Output = S> + Send + 'static,
    >(
        &mut self,
        mut streams: mpsc::UnboundedReceiver<SFut>,
        caster: impl Fn(S) -> Result<C, Error> + Send + Sync + Copy + 'static,
    ) {
        let idle_timeout = reflexo_typst::time::Duration::from_secs(5);
        let (conn_handler, shutdown_tx, mut shutdown_data_plane_rx) =
            self.data_plane_resources.take().unwrap();
        let (alive_tx, mut alive_rx) = mpsc::unbounded_channel::<()>();

        let recv = move |conn| {
            let h = conn_handler.clone();
            let alive_tx = alive_tx.clone();
            tokio::spawn(async move {
                let conn: C = caster(conn.await).unwrap();
                tokio::pin!(conn);

                if h.enable_partial_rendering {
                    conn.send(WsMessage::Binary("partial-rendering,true".into()))
                        .await
                        .unwrap();
                }
                if !h.invert_colors.is_empty() {
                    conn.send(WsMessage::Binary(
                        format!("invert-colors,{}", h.invert_colors).into(),
                    ))
                    .await
                    .unwrap();
                }
                let actor::webview::Channels { svg } =
                    actor::webview::WebviewActor::<'_, C>::set_up_channels();
                let webview_actor = actor::webview::WebviewActor::new(
                    conn,
                    svg.1,
                    h.webview_tx.clone(),
                    h.webview_tx.subscribe(),
                    h.editor_tx.clone(),
                    h.renderer_tx.clone(),
                );
                let render_actor = actor::render::RenderActor::new(
                    h.renderer_tx.subscribe(),
                    h.doc_sender.clone(),
                    h.typst_tx,
                    svg.0,
                    h.webview_tx,
                );
                tokio::spawn(render_actor.run());
                let outline_render_actor = actor::render::OutlineRenderActor::new(
                    h.renderer_tx.subscribe(),
                    h.doc_sender.clone(),
                    h.editor_tx.clone(),
                    h.span_interner,
                );
                tokio::spawn(outline_render_actor.run());

                struct FinallySend(mpsc::UnboundedSender<()>);
                impl Drop for FinallySend {
                    fn drop(&mut self) {
                        let _ = self.0.send(());
                    }
                }

                let _send = FinallySend(alive_tx);
                webview_actor.run().await;
            })
        };

        let data_plane_handle = tokio::spawn(async move {
            let mut alive_cnt = 0;
            let mut shutdown_bell = tokio::time::interval(idle_timeout);
            loop {
                if shutdown_tx.is_some() {
                    shutdown_bell.reset();
                }
                tokio::select! {
                    Some(()) = shutdown_data_plane_rx.recv() => {
                        log::info!("Data plane server shutdown");
                        return;
                    }
                    Some(stream) = streams.recv() => {
                        alive_cnt += 1;
                        tokio::spawn(recv(stream));
                    },
                    _ = alive_rx.recv() => {
                        alive_cnt -= 1;
                    }
                    _ = shutdown_bell.tick(), if alive_cnt == 0 && shutdown_tx.is_some() => {
                        let shutdown_tx = shutdown_tx.expect("scheduled shutdown without shutdown_tx");
                        log::info!(
                            "Data plane server has been idle for {idle_timeout:?}, shutting down."
                        );
                        let _ = shutdown_tx.send(()).await;
                        log::info!("Data plane server shutdown");
                        return;
                    }
                }
            }
        });

        self.data_plane_handle = Some(data_plane_handle);
    }
}

type MpScChannel<T> = (mpsc::UnboundedSender<T>, mpsc::UnboundedReceiver<T>);
type BroadcastChannel<T> = (broadcast::Sender<T>, broadcast::Receiver<T>);

pub struct PreviewBuilder {
    arguments: PreviewArgs,
    shutdown_tx: Option<mpsc::Sender<()>>,
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
            shutdown_tx: None,
            typst_mailbox: mpsc::unbounded_channel(),
            renderer_mailbox: broadcast::channel(1024),
            editor_conn: mpsc::unbounded_channel(),
            webview_conn: broadcast::channel(32),
            doc_sender: Arc::new(std::sync::RwLock::new(None)),
            compile_watcher: OnceCell::new(),
        }
    }

    pub fn with_shutdown_tx(mut self, shutdown_tx: mpsc::Sender<()>) -> Self {
        self.shutdown_tx = Some(shutdown_tx);
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

    pub async fn build<T>(self, conn: ControlPlaneTx, client: Arc<T>) -> Previewer
    where
        T: CompileHost + Send + Sync + 'static,
    {
        let PreviewBuilder {
            arguments,
            shutdown_tx,
            typst_mailbox,
            renderer_mailbox,
            editor_conn: (editor_tx, editor_rx),
            webview_conn: (webview_tx, _),
            doc_sender,
            ..
        } = self;

        // Shared resource
        let span_interner = SpanInterner::new();
        let (shutdown_data_plane_tx, shutdown_data_plane_rx) = mpsc::channel(1);

        // Spawns the typst actor
        let typst_actor = TypstActor::new(
            client,
            typst_mailbox.1,
            renderer_mailbox.0.clone(),
            editor_tx.clone(),
            webview_tx.clone(),
        );
        tokio::spawn(typst_actor.run());
        log::info!("Previewer: typst actor spawned");

        // Spawns the editor actor
        let editor_actor = EditorActor::new(
            editor_rx,
            conn,
            typst_mailbox.0.clone(),
            webview_tx.clone(),
            span_interner.clone(),
        );
        let control_plane_handle = tokio::spawn(editor_actor.run());
        log::info!("Previewer: editor actor spawned");

        // Delayed data plane binding
        let data_plane = DataPlane {
            span_interner: span_interner.clone(),
            webview_tx: webview_tx.clone(),
            typst_tx: typst_mailbox.0.clone(),
            editor_tx: editor_tx.clone(),
            invert_colors: arguments.invert_colors.clone(),
            renderer_tx: renderer_mailbox.0.clone(),
            enable_partial_rendering: arguments.enable_partial_rendering,
            doc_sender,
        };

        Previewer {
            control_plane_handle,
            data_plane_handle: None,
            data_plane_resources: Some((data_plane, shutdown_tx, shutdown_data_plane_rx)),
            stop: Some(Box::new(move || {
                Box::pin(async move {
                    let _ = shutdown_data_plane_tx.send(()).await;
                    let _ = editor_tx.send(EditorActorRequest::Shutdown);
                })
            })),
        }
    }
}

#[derive(Debug)]
pub enum WsMessage {
    /// A text WebSocket message
    Text(String),
    /// A binary WebSocket message
    Binary(Vec<u8>),
}

pub type SourceLocation = reflexo_typst::debug_loc::SourceLocation;

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

#[derive(Clone)]
struct DataPlane {
    span_interner: SpanInterner,
    webview_tx: broadcast::Sender<WebviewActorRequest>,
    typst_tx: mpsc::UnboundedSender<TypstActorRequest>,
    editor_tx: mpsc::UnboundedSender<EditorActorRequest>,
    enable_partial_rendering: bool,
    invert_colors: String,
    renderer_tx: broadcast::Sender<RenderActorRequest>,
    doc_sender: Arc<std::sync::RwLock<Option<Arc<Document>>>>,
}
