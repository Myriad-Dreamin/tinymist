use futures::{SinkExt, StreamExt};
use log::{debug, info, trace, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::{net::TcpStream, sync::broadcast};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};
use typst_ts_core::debug_loc::DocumentPosition;

use crate::debug_loc::{InternQuery, SpanInterner};
use crate::outline::Outline;
use crate::{
    actor::typst::TypstActorRequest, ChangeCursorPositionRequest, DocToSrcJumpInfo, MemoryFiles,
    MemoryFilesShort, SrcToDocJumpRequest,
};

use super::webview::WebviewActorRequest;
#[derive(Debug, Deserialize)]
pub struct DocToSrcJumpResolveRequest {
    /// Span id in hex-format.
    pub span: String,
}

#[derive(Debug, Deserialize)]
pub struct PanelScrollByPositionRequest {
    position: DocumentPosition,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum CompileStatus {
    Compiling,
    CompileSuccess,
    CompileError,
}

#[derive(Debug)]
pub enum EditorActorRequest {
    Shutdown,
    DocToSrcJumpResolve(DocToSrcJumpResolveRequest),
    DocToSrcJump(DocToSrcJumpInfo),
    Outline(Outline),
    CompileStatus(CompileStatus),
}

pub struct LspControlPlaneTx {
    pub resp_tx: mpsc::UnboundedSender<ControlPlaneResponse>,
    pub ctl_rx: mpsc::UnboundedReceiver<ControlPlaneMessage>,
    pub shutdown_tx: mpsc::Sender<()>,
}

pub struct LspControlPlaneRx {
    pub resp_rx: mpsc::UnboundedReceiver<ControlPlaneResponse>,
    pub ctl_tx: mpsc::UnboundedSender<ControlPlaneMessage>,
    pub shutdown_rx: mpsc::Receiver<()>,
}

impl LspControlPlaneTx {
    pub fn new() -> (LspControlPlaneTx, LspControlPlaneRx) {
        let (resp_tx, resp_rx) = mpsc::unbounded_channel();
        let (ctl_tx, ctl_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        (
            Self {
                resp_tx,
                ctl_rx,
                shutdown_tx,
            },
            LspControlPlaneRx {
                resp_rx,
                ctl_tx,
                shutdown_rx,
            },
        )
    }
}

pub enum EditorConnection {
    WebSocket(WebSocketStream<TcpStream>),
    Lsp(LspControlPlaneTx),
}

impl EditorConnection {
    fn need_sync_files(&self) -> bool {
        matches!(self, EditorConnection::WebSocket(_))
    }

    async fn sync_editor_changes(&mut self) {
        let EditorConnection::WebSocket(ws) = self else {
            return;
        };

        let Ok(_) = ws
            .send(Message::Text(
                serde_json::to_string(&ControlPlaneResponse::SyncEditorChanges(())).unwrap(),
            ))
            .await
        else {
            warn!("failed to send sync editor changes to editor");
            return;
        };
    }

    async fn resp_ctl_plane(&mut self, loc: &str, resp: ControlPlaneResponse) -> bool {
        let sent = match self {
            EditorConnection::WebSocket(ws) => ws
                .send(Message::Text(serde_json::to_string(&resp).unwrap()))
                .await
                .is_ok(),
            EditorConnection::Lsp(LspControlPlaneTx { resp_tx, .. }) => resp_tx.send(resp).is_ok(),
        };

        if !sent {
            warn!("failed to send {loc} response to editor");
        }

        sent
    }

    async fn next(&mut self) -> Option<ControlPlaneMessage> {
        match self {
            EditorConnection::Lsp(LspControlPlaneTx { ctl_rx, .. }) => ctl_rx.recv().await,
            EditorConnection::WebSocket(ws) => {
                let Some(Ok(Message::Text(msg))) = ws.next().await else {
                    return None;
                };

                let Ok(msg) = serde_json::from_str::<ControlPlaneMessage>(&msg) else {
                    warn!("failed to parse control plane request: {msg:?}");
                    return None;
                };

                Some(msg)
            }
        }
    }
}

pub struct EditorActor {
    mailbox: mpsc::UnboundedReceiver<EditorActorRequest>,
    editor_conn: EditorConnection,

    world_sender: mpsc::UnboundedSender<TypstActorRequest>,
    webview_sender: broadcast::Sender<WebviewActorRequest>,

    span_interner: SpanInterner,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "event")]
pub enum ControlPlaneMessage {
    #[serde(rename = "changeCursorPosition")]
    ChangeCursorPosition(ChangeCursorPositionRequest),
    #[serde(rename = "panelScrollTo")]
    SrcToDocJump(SrcToDocJumpRequest),
    #[serde(rename = "panelScrollByPosition")]
    PanelScrollByPosition(PanelScrollByPositionRequest),
    #[serde(rename = "sourceScrollBySpan")]
    DocToSrcJumpResolve(DocToSrcJumpResolveRequest),
    #[serde(rename = "syncMemoryFiles")]
    SyncMemoryFiles(MemoryFiles),
    #[serde(rename = "updateMemoryFiles")]
    UpdateMemoryFiles(MemoryFiles),
    #[serde(rename = "removeMemoryFiles")]
    RemoveMemoryFiles(MemoryFilesShort),
}

#[derive(Debug, Serialize)]
#[serde(tag = "event")]
pub enum ControlPlaneResponse {
    #[serde(rename = "editorScrollTo")]
    EditorScrollTo(DocToSrcJumpInfo),
    #[serde(rename = "syncEditorChanges")]
    SyncEditorChanges(()),
    #[serde(rename = "compileStatus")]
    CompileStatus(CompileStatus),
    #[serde(rename = "outline")]
    Outline(Outline),
}

impl EditorActor {
    pub fn new(
        mailbox: mpsc::UnboundedReceiver<EditorActorRequest>,
        editor_websocket_conn: EditorConnection,
        world_sender: mpsc::UnboundedSender<TypstActorRequest>,
        webview_sender: broadcast::Sender<WebviewActorRequest>,
        span_interner: SpanInterner,
    ) -> Self {
        Self {
            mailbox,
            editor_conn: editor_websocket_conn,
            world_sender,
            webview_sender,

            span_interner,
        }
    }

    pub async fn run(mut self) {
        if self.editor_conn.need_sync_files() {
            self.editor_conn.sync_editor_changes().await;
        }

        loop {
            tokio::select! {
                Some(msg) = self.mailbox.recv() => {
                    trace!("EditorActor: received message from mailbox: {:?}", msg);
                   let sent = match msg {
                        EditorActorRequest::Shutdown => {
                            info!("EditorActor: received exit message");
                            break;
                        },
                        EditorActorRequest::DocToSrcJump(jump_info) => {
                            self.editor_conn.resp_ctl_plane("DocToSrcJump", ControlPlaneResponse::EditorScrollTo(jump_info)).await
                        },
                        EditorActorRequest::DocToSrcJumpResolve(req) => {
                            self.source_scroll_by_span(req.span)
                                .await;

                            false
                        },
                        EditorActorRequest::CompileStatus(status) => {
                            self.editor_conn.resp_ctl_plane("CompileStatus", ControlPlaneResponse::CompileStatus(status)).await
                        },
                        EditorActorRequest::Outline(outline) => {
                            self.editor_conn.resp_ctl_plane("Outline", ControlPlaneResponse::Outline(outline)).await
                        }
                    };

                    if !sent {
                        break;
                    }
                }
                Some(msg) = self.editor_conn.next() => {
                    match msg {
                        ControlPlaneMessage::ChangeCursorPosition(cursor_info) => {
                            debug!("EditorActor: received message from editor: {:?}", cursor_info);
                            self.world_sender.send(TypstActorRequest::ChangeCursorPosition(cursor_info)).unwrap();
                        }
                        ControlPlaneMessage::SrcToDocJump(jump_info) => {
                            debug!("EditorActor: received message from editor: {:?}", jump_info);
                            self.world_sender.send(TypstActorRequest::SrcToDocJumpResolve(jump_info)).unwrap();
                        }
                        ControlPlaneMessage::PanelScrollByPosition(jump_info) => {
                            debug!("EditorActor: received message from editor: {:?}", jump_info);
                            self.webview_sender.send(WebviewActorRequest::ViewportPosition(jump_info.position)).unwrap();
                        }
                        ControlPlaneMessage::DocToSrcJumpResolve(jump_info) => {
                            debug!("EditorActor: received message from editor: {:?}", jump_info);

                            self.source_scroll_by_span(jump_info.span)
                                .await;
                        }
                        ControlPlaneMessage::SyncMemoryFiles(memory_files) => {
                            debug!("EditorActor: received message from editor: SyncMemoryFiles {:?}", memory_files.files.keys().collect::<Vec<_>>());
                            self.world_sender.send(TypstActorRequest::SyncMemoryFiles(memory_files)).unwrap();
                        }
                        ControlPlaneMessage::UpdateMemoryFiles(memory_files) => {
                            debug!("EditorActor: received message from editor: UpdateMemoryFiles {:?}", memory_files.files.keys().collect::<Vec<_>>());
                            self.world_sender.send(TypstActorRequest::UpdateMemoryFiles(memory_files)).unwrap();
                        }
                        ControlPlaneMessage::RemoveMemoryFiles(memory_files) => {
                            debug!("EditorActor: received message from editor: RemoveMemoryFiles {:?}", &memory_files.files);
                            self.world_sender.send(TypstActorRequest::RemoveMemoryFiles(memory_files)).unwrap();
                        }
                    };
                }
            }
        }

        info!("EditorActor: editor disconnected");

        if !matches!(self.editor_conn, EditorConnection::Lsp(_)) {
            info!("EditorActor: shutting down whole program");
            std::process::exit(0);
        }
    }

    async fn source_scroll_by_span(&mut self, span: String) {
        let jump_info = {
            match self.span_interner.span_by_str(&span).await {
                InternQuery::Ok(s) => s,
                InternQuery::UseAfterFree => {
                    warn!("EditorActor: out of date span id: {}", span);
                    return;
                }
            }
        };
        if let Some(span) = jump_info {
            let span_and_offset = span.into();
            self.world_sender
                .send(TypstActorRequest::DocToSrcJumpResolve((
                    span_and_offset,
                    span_and_offset,
                )))
                .unwrap();
        };
    }
}
