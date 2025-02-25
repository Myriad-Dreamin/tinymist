use futures::{SinkExt, StreamExt};
use reflexo_typst::debug_loc::{DocumentPosition, ElementPoint};
use tinymist_std::error::IgnoreLogging;
use tokio::sync::{broadcast, mpsc};

use crate::{
    actor::{editor::DocToSrcJumpResolveRequest, render::ResolveSpanRequest},
    Message, WsError,
};

use super::{editor::EditorActorRequest, render::RenderActorRequest};

// pub type CursorPosition = DocumentPosition;
pub type SrcToDocJumpInfo = DocumentPosition;

#[derive(Debug, Clone)]
pub enum WebviewActorRequest {
    ViewportPosition(DocumentPosition),
    SrcToDocJump(Vec<SrcToDocJumpInfo>),
    // CursorPosition(CursorPosition),
    CursorPaths(Vec<Vec<ElementPoint>>),
}

fn position_req(
    event: &'static str,
    DocumentPosition { page_no, x, y }: DocumentPosition,
) -> String {
    format!("{event},{page_no} {x} {y}")
}

fn positions_req(event: &'static str, positions: Vec<DocumentPosition>) -> String {
    format!("{event},")
        + &positions
            .iter()
            .map(|DocumentPosition { page_no, x, y }| format!("{page_no} {x} {y}"))
            .collect::<Vec<_>>()
            .join(",")
}

pub struct WebviewActor<
    'a,
    C: futures::Sink<Message, Error = WsError> + futures::Stream<Item = Result<Message, WsError>>,
> {
    webview_websocket_conn: std::pin::Pin<&'a mut C>,
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

impl<
        'a,
        C: futures::Sink<Message, Error = WsError> + futures::Stream<Item = Result<Message, WsError>>,
    > WebviewActor<'a, C>
{
    pub fn set_up_channels() -> Channels {
        Channels {
            svg: mpsc::unbounded_channel(),
        }
    }
    pub fn new(
        websocket_conn: std::pin::Pin<&'a mut C>,
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

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                Ok(msg) = self.mailbox.recv() => {
                    log::trace!("WebviewActor: received message from mailbox: {:?}", msg);
                    match msg {
                        WebviewActorRequest::SrcToDocJump(jump_info) => {
                            let msg = positions_req("jump", jump_info);
                            self.webview_websocket_conn.send(Message::Binary(msg.into_bytes()))
                              .await.log_error("WebViewActor");
                        }
                        WebviewActorRequest::ViewportPosition(jump_info) => {
                            let msg = position_req("viewport", jump_info);
                            self.webview_websocket_conn.send(Message::Binary(msg.into_bytes()))
                              .await.log_error("WebViewActor");
                        }
                        // WebviewActorRequest::CursorPosition(jump_info) => {
                        //     let msg = position_req("cursor", jump_info);
                        //     self.webview_websocket_conn.send(WsMessage::Binary(msg.into_bytes())).await.log_error("WebViewActor");
                        // }
                        WebviewActorRequest::CursorPaths(jump_info) => {
                            let json = serde_json::to_string(&jump_info).unwrap();
                            let msg = format!("cursor-paths,{json}");
                            self.webview_websocket_conn.send(Message::Binary(msg.into_bytes()))
                              .await.log_error("WebViewActor");
                        }
                    }
                }
                Some(svg) = self.svg_receiver.recv() => {
                    log::trace!("WebviewActor: received svg from renderer");
                    self.webview_websocket_conn.send(Message::Binary(svg))
                    .await.log_error("WebViewActor");
                }
                Some(msg) = self.webview_websocket_conn.next() => {
                    log::trace!("WebviewActor: received message from websocket: {:?}", msg);
                    let Ok(msg) = msg else {
                        log::info!("WebviewActor: no more messages from websocket: {}", msg.unwrap_err());
                      break;
                    };
                    let Message::Text(msg) = msg else {
                        log::info!("WebviewActor: received non-text message from websocket: {:?}", msg);
                        let _ = self.webview_websocket_conn.send(Message::Text(format!("Webview Actor: error, received non-text message: {msg:?}")))
                        .await;
                        break;
                    };
                    if msg == "current" {
                        self.render_sender.send(RenderActorRequest::RenderFullLatest).log_error("WebViewActor");
                    } else if msg.starts_with("srclocation") {
                        let location = msg.split(' ').nth(1).unwrap();
                        self.editor_sender.send(EditorActorRequest::DocToSrcJumpResolve(
                            DocToSrcJumpResolveRequest {
                                span: location.trim().to_owned(),
                            },
                        )).log_error("WebViewActor");
                    } else if msg.starts_with("outline-sync") {
                        let location = msg.split(',').nth(1).unwrap();
                        let location = location.split(' ').collect::<Vec::<&str>>();
                        let page_no = location[0].parse().unwrap();
                        let x = location.get(1).map(|s| s.parse().unwrap()).unwrap_or(0.);
                        let y = location.get(2).map(|s| s.parse().unwrap()).unwrap_or(0.);
                        let pos = DocumentPosition { page_no, x, y };

                        self.broadcast_sender.send(WebviewActorRequest::ViewportPosition(pos)).log_error("WebViewActor");
                    } else if msg.starts_with("srcpath") {
                        let path = msg.split(' ').nth(1).unwrap();
                        let path = serde_json::from_str(path);
                        if let Ok(path) = path {
                            let path: Vec<(u32, u32, String)> = path;
                            let path = path.into_iter().map(ElementPoint::from).collect::<Vec<_>>();
                            self.render_sender.send(RenderActorRequest::WebviewResolveSpan(ResolveSpanRequest(path))).log_error("WebViewActor");
                        };
                    } else if msg.starts_with("src-point") {
                        let path = msg.split(' ').nth(1).unwrap();
                        let path = serde_json::from_str(path);
                        if let Ok(path) = path {
                            self.render_sender.send(RenderActorRequest::WebviewResolveFrameLoc(path)).log_error("WebViewActor");
                        };
                    } else {
                        let err = self.webview_websocket_conn.send(Message::Text(format!("error, received unknown message: {msg}"))).await;
                        log::info!("WebviewActor: received unknown message from websocket: {msg} {err:?}");
                        break;
                    }
                }
                else => {
                    break;
                }
            }
        }
        log::info!("WebviewActor: exiting");
    }
}
