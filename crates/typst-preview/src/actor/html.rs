use std::sync::Arc;

use tinymist_std::typst::TypstDocument;
use tokio::sync::{broadcast, mpsc};

use super::{render::RenderActorRequest, webview::PreviewFrame};
use crate::CompileView;

pub struct HtmlRenderActor {
    mailbox: broadcast::Receiver<RenderActorRequest>,
    view: Arc<parking_lot::RwLock<Option<Arc<dyn CompileView>>>>,
    frame_sender: mpsc::UnboundedSender<PreviewFrame>,
}

impl HtmlRenderActor {
    pub fn new(
        mailbox: broadcast::Receiver<RenderActorRequest>,
        view: Arc<parking_lot::RwLock<Option<Arc<dyn CompileView>>>>,
        frame_sender: mpsc::UnboundedSender<PreviewFrame>,
    ) -> Self {
        Self {
            mailbox,
            view,
            frame_sender,
        }
    }

    pub async fn run(mut self) {
        loop {
            match self.mailbox.recv().await {
                Ok(
                    RenderActorRequest::RenderFullLatest | RenderActorRequest::RenderIncremental,
                ) => {}
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Closed) => {
                    log::info!("HtmlRenderActor: no more messages");
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    log::info!("HtmlRenderActor: lagged message. Some events are dropped");
                }
            }

            while let Ok(msg) = self.mailbox.try_recv() {
                if !matches!(
                    msg,
                    RenderActorRequest::RenderFullLatest | RenderActorRequest::RenderIncremental
                ) {
                    continue;
                }
            }

            let Some(document) = self.view.read().as_ref().and_then(|view| view.doc()) else {
                log::info!("HtmlRenderActor: document is not ready");
                continue;
            };

            let TypstDocument::Html(document) = &document else {
                continue;
            };

            let frame = match typst_html::html(document, &typst_html::HtmlOptions::default()) {
                Ok(html) => PreviewFrame::Html(html.into_bytes()),
                Err(err) => {
                    log::warn!("failed to encode HTML preview document: {err:?}");
                    PreviewFrame::HtmlError(b"failed to encode HTML preview document".to_vec())
                }
            };

            if self.frame_sender.send(frame).is_err() {
                log::info!("HtmlRenderActor: frame_sender is dropped");
                break;
            }
        }

        log::info!("HtmlRenderActor: exiting");
    }
}
