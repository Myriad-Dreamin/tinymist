use futures::{SinkExt, StreamExt};
use reflexo_typst::debug_loc::DocumentPosition;
use tinymist_std::error::IgnoreLogging;
use tokio::sync::{broadcast, mpsc};

use super::{editor::EditorActorRequest, render::RenderActorRequest};
use crate::{ViewerWindowStateMessage, WsMessage, actor::editor::DocToSrcJumpResolveRequest};

// pub type CursorPosition = DocumentPosition;
pub type SrcToDocJumpInfo = DocumentPosition;

#[derive(Debug, Clone)]
pub enum WebviewActorRequest {
    ViewportPosition(DocumentPosition),
    SrcToDocJump(Vec<SrcToDocJumpInfo>),
    // CursorPosition(CursorPosition),
}

#[derive(Debug)]
pub enum PreviewFrame {
    Paged(Vec<u8>),
    Html(Vec<u8>),
    HtmlError(Vec<u8>),
}

impl PreviewFrame {
    fn into_message(self) -> Vec<u8> {
        match self {
            PreviewFrame::Paged(frame) => frame,
            PreviewFrame::Html(html) => prefixed_frame(b"html,", html),
            PreviewFrame::HtmlError(error) => prefixed_frame(b"html-error,", error),
        }
    }
}

fn prefixed_frame(prefix: &[u8], payload: Vec<u8>) -> Vec<u8> {
    let mut frame = Vec::with_capacity(prefix.len() + payload.len());
    frame.extend_from_slice(prefix);
    frame.extend_from_slice(&payload);
    frame
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

pub struct WebviewActor<'a, C> {
    webview_websocket_conn: std::pin::Pin<&'a mut C>,
    frame_receiver: mpsc::UnboundedReceiver<PreviewFrame>,
    mailbox: broadcast::Receiver<WebviewActorRequest>,

    broadcast_sender: broadcast::Sender<WebviewActorRequest>,
    editor_sender: mpsc::UnboundedSender<EditorActorRequest>,
    render_sender: broadcast::Sender<RenderActorRequest>,
}

pub struct Channels {
    pub frame: (
        mpsc::UnboundedSender<PreviewFrame>,
        mpsc::UnboundedReceiver<PreviewFrame>,
    ),
}

impl<'a, C> WebviewActor<'a, C>
where
    C: futures::Sink<WsMessage, Error = reflexo_typst::Error>
        + futures::Stream<Item = Result<WsMessage, reflexo_typst::Error>>,
{
    pub fn set_up_channels() -> Channels {
        Channels {
            frame: mpsc::unbounded_channel(),
        }
    }
    pub fn new(
        websocket_conn: std::pin::Pin<&'a mut C>,
        frame_receiver: mpsc::UnboundedReceiver<PreviewFrame>,
        broadcast_sender: broadcast::Sender<WebviewActorRequest>,
        mailbox: broadcast::Receiver<WebviewActorRequest>,
        editor_sender: mpsc::UnboundedSender<EditorActorRequest>,
        render_sender: broadcast::Sender<RenderActorRequest>,
    ) -> Self {
        Self {
            webview_websocket_conn: websocket_conn,
            frame_receiver,
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
                    log::trace!("WebviewActor: received message from mailbox: {msg:?}");
                    match msg {
                        WebviewActorRequest::SrcToDocJump(jump_info) => {
                            let msg = positions_req("jump", jump_info);
                            self.webview_websocket_conn.send(WsMessage::Binary(msg.into()))
                              .await.log_error("WebViewActor");
                        }
                        WebviewActorRequest::ViewportPosition(jump_info) => {
                            let msg = position_req("viewport", jump_info);
                            self.webview_websocket_conn.send(WsMessage::Binary(msg.into()))
                              .await.log_error("WebViewActor");
                        }
                    }
                }
                Some(frame) = self.frame_receiver.recv() => {
                    log::trace!("WebviewActor: received preview frame from renderer");
                    let _scope = typst_timing::TimingScope::new("webview_actor_send_frame");
                    self.webview_websocket_conn.send(WsMessage::Binary(frame.into_message().into()))
                    .await.log_error("WebViewActor");
                }
                Some(msg) = self.webview_websocket_conn.next() => {
                    log::trace!("WebviewActor: received message from websocket: {msg:?}");
                    let Ok(msg) = msg else {
                        log::info!("WebviewActor: no more messages from websocket: {}", msg.unwrap_err());
                      break;
                    };
                    let msg = match msg {
                        WsMessage::Text(msg) => msg,
                        WsMessage::Ping(msg) => {
                            let _ = self.webview_websocket_conn.send(WsMessage::Pong(msg)).await;
                            continue;
                        },
                        WsMessage::Pong(..) => {
                            continue;
                        },
                        _ =>  {
                            log::info!("WebviewActor: received non-text message from websocket: {msg:?}");
                            let _ = self.webview_websocket_conn.send(WsMessage::Text(format!("Webview Actor: error, received non-text message: {msg:?}")))
                            .await;
                            break;
                        }
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
                    } else if msg.starts_with("src-point") {
                        let path = msg.split(' ').nth(1).unwrap();
                        let path = serde_json::from_str(path);
                        if let Ok(path) = path {
                            self.render_sender.send(RenderActorRequest::WebviewResolveFrameLoc(path)).log_error("WebViewActor");
                        };
                    } else if let Some(state) = msg.strip_prefix("viewer-window-state ") {
                        if let Ok(state) = serde_json::from_str::<ViewerWindowStateMessage>(state) {
                            self.editor_sender.send(EditorActorRequest::ViewerWindowState(state)).log_error("WebViewActor");
                        };
                    } else {
                        let err = self.webview_websocket_conn.send(WsMessage::Text(format!("error, received unknown message: {msg}"))).await;
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
