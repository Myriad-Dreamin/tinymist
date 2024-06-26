use std::sync::Arc;

use await_tree::InstrumentAwait;
use log::{debug, info, trace};
use tokio::sync::{broadcast, mpsc, watch};
use typst::model::Document;
use typst_ts_core::debug_loc::{ElementPoint, SourceSpanOffset};
use typst_ts_core::TypstDocument;
use typst_ts_svg_exporter::IncrSvgDocServer;

use crate::await_tree::REGISTRY;
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
    document: watch::Receiver<Option<Arc<Document>>>,
    renderer: IncrSvgDocServer,
    resolve_sender: mpsc::UnboundedSender<TypstActorRequest>,
    svg_sender: mpsc::UnboundedSender<Vec<u8>>,
    webview_sender: broadcast::Sender<WebviewActorRequest>,
}

impl RenderActor {
    pub fn new(
        mailbox: broadcast::Receiver<RenderActorRequest>,
        document: watch::Receiver<Option<Arc<Document>>>,
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

    pub fn spawn(self, peer_addr: String) {
        std::thread::Builder::new()
            .name("RenderActor".to_owned())
            .spawn(move || self.run(&peer_addr))
            .unwrap();
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

    #[tokio::main(flavor = "current_thread")]
    async fn run(self, peer_addr: &str) {
        let span = format!("render actor<{}>", peer_addr);
        let root = REGISTRY.lock().await.register(span.clone().into(), span);
        root.instrument(self.run_instrumented()).await;
    }

    async fn run_instrumented(mut self) {
        loop {
            let mut has_full_render = false;
            debug!("RenderActor: waiting for message");
            match self
                .mailbox
                .recv()
                .instrument_await("waiting for message")
                .await
            {
                Ok(msg) => {
                    has_full_render |= self
                        .process_message(msg)
                        .instrument_await("processing message")
                        .await;
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
                has_full_render |= self
                    .process_message(msg)
                    .instrument_await("processing message")
                    .await;
            }
            // if a full render is requested, we render the latest document
            // otherwise, we render the incremental changes for only once
            let has_full_render = has_full_render;
            debug!("RenderActor: has_full_render: {}", has_full_render);
            let Some(document) = self.document.borrow().clone() else {
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
    document: watch::Receiver<Option<Arc<Document>>>,
    editor_tx: mpsc::UnboundedSender<EditorActorRequest>,

    span_interner: SpanInterner,
}

impl OutlineRenderActor {
    pub fn new(
        signal: broadcast::Receiver<RenderActorRequest>,
        document: watch::Receiver<Option<Arc<Document>>>,
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

    pub fn spawn(self, peer_addr: String) {
        std::thread::Builder::new()
            .name("OutlineRenderActor".to_owned())
            .spawn(move || self.run(&peer_addr))
            .unwrap();
    }

    #[tokio::main(flavor = "current_thread")]
    async fn run(self, peer_addr: &str) {
        let span = format!("outline render actor<{}>", peer_addr);
        let root = REGISTRY.lock().await.register(span.clone().into(), span);
        root.instrument(self.run_instrumented()).await;
    }

    async fn run_instrumented(mut self) {
        loop {
            debug!("OutlineRenderActor: waiting for message");
            match self
                .signal
                .recv()
                .instrument_await("waiting for message")
                .await
            {
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
            let Some(document) = self.document.borrow().clone() else {
                info!("OutlineRenderActor: document is not ready");
                continue;
            };
            let data = self.outline(&document).instrument_await("outline").await;
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
            .instrument_await("generating outline with span interner")
            .await
    }
}
