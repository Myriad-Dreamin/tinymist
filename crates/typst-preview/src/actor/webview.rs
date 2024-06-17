use await_tree::InstrumentAwait;
use futures::{SinkExt, StreamExt};
use log::{info, trace};
use tokio::{
    net::TcpStream,
    sync::{broadcast, mpsc},
};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};
use typst_ts_core::debug_loc::{DocumentPosition, ElementPoint};

use crate::{
    actor::{editor::DocToSrcJumpResolveRequest, render::ResolveSpanRequest},
    await_tree::REGISTRY,
};

use super::{editor::EditorActorRequest, render::RenderActorRequest};

// pub type CursorPosition = DocumentPosition;
pub type SrcToDocJumpInfo = DocumentPosition;

#[derive(Debug, Clone)]
pub enum WebviewActorRequest {
    ViewportPosition(DocumentPosition),
    SrcToDocJump(SrcToDocJumpInfo),
    // CursorPosition(CursorPosition),
    CursorPaths(Vec<Vec<ElementPoint>>),
}

fn position_req(
    event: &'static str,
    DocumentPosition { page_no, x, y }: DocumentPosition,
) -> String {
    format!("{event},{page_no} {x} {y}")
}

pub struct WebviewActor {
    webview_websocket_conn: WebSocketStream<TcpStream>,
    svg_receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    mailbox: broadcast::Receiver<WebviewActorRequest>,

    broadcast_sender: broadcast::Sender<WebviewActorRequest>,
    editor_sender: mpsc::UnboundedSender<EditorActorRequest>,
    render_sender: broadcast::Sender<RenderActorRequest>,
}

pub struct Channels {
    pub svg: (
        mpsc::UnboundedSender<Vec<u8>>,
        mpsc::UnboundedReceiver<Vec<u8>>,
    ),
}

impl WebviewActor {
    pub fn set_up_channels() -> Channels {
        Channels {
            svg: mpsc::unbounded_channel(),
        }
    }
    pub fn new(
        websocket_conn: WebSocketStream<TcpStream>,
        svg_receiver: mpsc::UnboundedReceiver<Vec<u8>>,
        broadcast_sender: broadcast::Sender<WebviewActorRequest>,
        mailbox: broadcast::Receiver<WebviewActorRequest>,
        editor_sender: mpsc::UnboundedSender<EditorActorRequest>,
        render_sender: broadcast::Sender<RenderActorRequest>,
    ) -> Self {
        Self {
            webview_websocket_conn: websocket_conn,
            svg_receiver,
            mailbox,
            broadcast_sender,
            editor_sender,
            render_sender,
        }
    }

    pub async fn run(self, peer_addr: String) {
        let span = format!("webview actor<{}>", peer_addr);
        let root = REGISTRY.lock().await.register(span.clone().into(), span);
        root.instrument(self.run_instrumented()).await;
    }

    pub async fn run_instrumented(mut self) {
        loop {
            tokio::select! {
                Ok(msg) = self.mailbox.recv().instrument_await("waiting for mailbox") => {
                    trace!("WebviewActor: received message from mailbox: {:?}", msg);
                    match msg {
                        WebviewActorRequest::SrcToDocJump(jump_info) => {
                            let msg = position_req("jump", jump_info);
                            self.webview_websocket_conn.send(Message::Binary(msg.into_bytes()))
                            .instrument_await("send SrcToDocJump message to webview")
                            .await.unwrap();
                        }
                        WebviewActorRequest::ViewportPosition(jump_info) => {
                            let msg = position_req("viewport", jump_info);
                            self.webview_websocket_conn.send(Message::Binary(msg.into_bytes()))
                            .instrument_await("send ViewportPosition message to webview")
                            .await.unwrap();
                        }
                        // WebviewActorRequest::CursorPosition(jump_info) => {
                        //     let msg = position_req("cursor", jump_info);
                        //     self.webview_websocket_conn.send(Message::Binary(msg.into_bytes())).await.unwrap();
                        // }
                        WebviewActorRequest::CursorPaths(jump_info) => {
                            let json = serde_json::to_string(&jump_info).unwrap();
                            let msg = format!("cursor-paths,{json}");
                            self.webview_websocket_conn.send(Message::Binary(msg.into_bytes()))
                            .instrument_await("send CursorPaths message to webview")
                            .await.unwrap();
                        }
                    }
                }
                Some(svg) = self.svg_receiver.recv().instrument_await("waiting for renderer") => {
                    trace!("WebviewActor: received svg from renderer");
                    self.webview_websocket_conn.send(Message::Binary(svg))
                    .instrument_await("send svg to webview")
                    .await.unwrap();
                }
                Some(msg) = self.webview_websocket_conn.next().instrument_await("waiting for websocket") => {
                    trace!("WebviewActor: received message from websocket: {:?}", msg);
                    let Ok(msg) = msg else {
                        info!("WebviewActor: no more messages from websocket: {}", msg.unwrap_err());
                      break;
                    };
                    let Message::Text(msg) = msg else {
                        info!("WebviewActor: received non-text message from websocket: {:?}", msg);
                        let _ = self.webview_websocket_conn.send(Message::Text(format!("Webview Actor: error, received non-text message: {}", msg)))
                        .instrument_await("send error message to webview")
                        .await;
                        break;
                    };
                    if msg == "current" {
                        self.render_sender.send(RenderActorRequest::RenderFullLatest).unwrap();
                    } else if msg.starts_with("srclocation") {
                        let location = msg.split(' ').nth(1).unwrap();
                        self.editor_sender.send(EditorActorRequest::DocToSrcJumpResolve(
                            DocToSrcJumpResolveRequest {
                                span: location.trim().to_owned(),
                            },
                        )).unwrap();
                    } else if msg.starts_with("outline-sync") {
                        let location = msg.split(',').nth(1).unwrap();
                        let location = location.split(' ').collect::<Vec::<&str>>();
                        let page_no = location[0].parse().unwrap();
                        let x = location.get(1).map(|s| s.parse().unwrap()).unwrap_or(0.);
                        let y = location.get(2).map(|s| s.parse().unwrap()).unwrap_or(0.);
                        let pos = DocumentPosition { page_no, x, y };

                        self.broadcast_sender.send(WebviewActorRequest::ViewportPosition(pos)).unwrap();
                    } else if msg.starts_with("srcpath") {
                        let path = msg.split(' ').nth(1).unwrap();
                        let path = serde_json::from_str(path);
                        if let Ok(path) = path {
                            let path: Vec<(u32, u32, String)> = path;
                            let path = path.into_iter().map(ElementPoint::from).collect::<Vec<_>>();
                            self.render_sender.send(RenderActorRequest::ResolveSpan(ResolveSpanRequest(path))).unwrap();
                        };
                    } else {
                        info!("WebviewActor: received unknown message from websocket: {}", msg);
                        self.webview_websocket_conn.send(Message::Text(format!("error, received unknown message: {}", msg)))
                        .instrument_await("send error message to webview")
                        .await.unwrap();
                        break;
                    }
                }
                else => {
                    break;
                }
            }
        }
        info!("WebviewActor: exiting");
    }
}
