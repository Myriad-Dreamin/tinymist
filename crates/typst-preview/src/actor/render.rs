use std::sync::Arc;

use log::{debug, info, trace};
use reflexo_typst::debug_loc::{ElementPoint, SourceSpanOffset};
use reflexo_typst::TypstDocument;
use reflexo_vec2svg::IncrSvgDocServer;
use tokio::sync::{broadcast, mpsc};

use crate::{debug_loc::SpanInterner, outline::Outline};

use super::{editor::EditorActorRequest, typst::TypstActorRequest, webview::WebviewActorRequest};

#[derive(Debug, Clone)]
pub struct ResolveSpanRequest(pub Vec<ElementPoint>);

#[derive(Debug, Clone)]
pub enum RenderActorRequest {
    RenderFullLatest,
    RenderIncremental,
    ResolveSpan(ResolveSpanRequest),
    ChangeCursorPosition(SourceSpanOffset),
}

impl RenderActorRequest {
    pub fn is_full_render(&self) -> bool {
        match self {
            Self::RenderFullLatest => true,
            Self::RenderIncremental => false,
            Self::ResolveSpan(_) => false,
            Self::ChangeCursorPosition(_) => false,
        }
    }
}

pub struct RenderActor {
    mailbox: broadcast::Receiver<RenderActorRequest>,
    document: Arc<std::sync::RwLock<Option<TypstDocument>>>,
    renderer: IncrSvgDocServer,
    resolve_sender: mpsc::UnboundedSender<TypstActorRequest>,
    svg_sender: mpsc::UnboundedSender<Vec<u8>>,
    webview_sender: broadcast::Sender<WebviewActorRequest>,
}

impl RenderActor {
    pub fn new(
        mailbox: broadcast::Receiver<RenderActorRequest>,
        document: Arc<std::sync::RwLock<Option<TypstDocument>>>,
        resolve_sender: mpsc::UnboundedSender<TypstActorRequest>,
        svg_sender: mpsc::UnboundedSender<Vec<u8>>,
        webview_sender: broadcast::Sender<WebviewActorRequest>,
    ) -> Self {
        let mut res = Self {
            mailbox,
            document,
            renderer: IncrSvgDocServer::default(),
            resolve_sender,
            svg_sender,
            webview_sender,
        };
        res.renderer.set_should_attach_debug_info(true);
        res
    }

    async fn process_message(&mut self, msg: RenderActorRequest) -> bool {
        trace!("RenderActor: received message: {:?}", msg);

        let res = msg.is_full_render();

        match msg {
            RenderActorRequest::ResolveSpan(ResolveSpanRequest(element_path)) => {
                info!("RenderActor: resolving span: {:?}", element_path);
                let spans = match self.renderer.resolve_span_by_element_path(&element_path) {
                    Ok(spans) => spans,
                    Err(e) => {
                        info!("RenderActor: failed to resolve span: {}", e);
                        return false;
                    }
                };

                info!("RenderActor: resolved span: {:?}", spans);
                // end position is used
                if let Some(spans) = spans {
                    let Ok(_) = self
                        .resolve_sender
                        .send(TypstActorRequest::DocToSrcJumpResolve(spans))
                    else {
                        info!("RenderActor: resolve_sender is dropped");
                        return false;
                    };
                }
            }
            RenderActorRequest::ChangeCursorPosition(span_offset) => {
                info!("RenderActor: changing cursor position: {:?}", span_offset);

                let res = self.renderer.resolve_element_paths_by_span(span_offset);

                info!("RenderActor: resolved element paths: {:?}", res);
                if let Ok(info) = res {
                    let _ = self
                        .webview_sender
                        .send(WebviewActorRequest::CursorPaths(info));
                }
            }
            RenderActorRequest::RenderFullLatest | RenderActorRequest::RenderIncremental => {}
        }

        res
    }

    pub async fn run(mut self) {
        loop {
            let mut has_full_render = false;
            debug!("RenderActor: waiting for message");
            match self.mailbox.recv().await {
                Ok(msg) => {
                    has_full_render |= self.process_message(msg).await;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("RenderActor: no more messages");
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    info!("RenderActor: lagged message. Some events are dropped");
                }
            }
            // read the queue to empty
            while let Ok(msg) = self.mailbox.try_recv() {
                has_full_render |= self.process_message(msg).await;
            }
            // if a full render is requested, we render the latest document
            // otherwise, we render the incremental changes for only once
            let has_full_render = has_full_render;
            debug!("RenderActor: has_full_render: {has_full_render}");
            let Some(document) = self.document.read().unwrap().clone() else {
                info!("RenderActor: document is not ready");
                continue;
            };
            let data = if has_full_render {
                if let Some(data) = self.renderer.pack_current() {
                    data
                } else {
                    self.renderer.pack_delta(document)
                }
            } else {
                self.renderer.pack_delta(document)
            };
            let Ok(_) = self.svg_sender.send(data) else {
                info!("RenderActor: svg_sender is dropped");
                break;
            };
        }
        info!("RenderActor: exiting")
    }
}

pub struct OutlineRenderActor {
    signal: broadcast::Receiver<RenderActorRequest>,
    document: Arc<std::sync::RwLock<Option<TypstDocument>>>,
    editor_tx: mpsc::UnboundedSender<EditorActorRequest>,

    span_interner: SpanInterner,
}

impl OutlineRenderActor {
    pub fn new(
        signal: broadcast::Receiver<RenderActorRequest>,
        document: Arc<std::sync::RwLock<Option<TypstDocument>>>,
        editor_tx: mpsc::UnboundedSender<EditorActorRequest>,
        span_interner: SpanInterner,
    ) -> Self {
        Self {
            signal,
            document,
            editor_tx,
            span_interner,
        }
    }

    pub async fn run(mut self) {
        loop {
            debug!("OutlineRenderActor: waiting for message");
            match self.signal.recv().await {
                Ok(msg) => {
                    debug!("OutlineRenderActor: received message: {:?}", msg);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("OutlineRenderActor: no more messages");
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    info!("OutlineRenderActor: lagged message. Some events are dropped");
                }
            }
            // read the queue to empty
            while self.signal.try_recv().is_ok() {}
            // if a full render is requested, we render the latest document
            // otherwise, we render the incremental changes for only once
            let Some(document) = self.document.read().unwrap().clone() else {
                info!("OutlineRenderActor: document is not ready");
                continue;
            };
            let data = self.outline(&document).await;
            debug!("OutlineRenderActor: sending outline");
            let Ok(_) = self.editor_tx.send(EditorActorRequest::Outline(data)) else {
                info!("OutlineRenderActor: outline_sender is dropped");
                break;
            };
        }
        info!("OutlineRenderActor: exiting")
    }

    async fn outline(&self, document: &TypstDocument) -> Outline {
        self.span_interner
            .with_writer(|interner| {
                interner.reset();
                crate::outline::outline(interner, document)
            })
            .await
    }
}
