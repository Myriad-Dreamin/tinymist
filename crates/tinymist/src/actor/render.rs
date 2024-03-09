//! The (PDF) render actor

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use log::info;
use tokio::sync::{
    broadcast::{self, error::RecvError},
    watch,
};
use typst::foundations::Smart;
use typst_ts_core::TypstDocument;

use crate::ExportPdfMode;

#[derive(Debug, Clone)]
pub enum RenderActorRequest {
    Render,
    // ChangeConfig(PdfExportConfig),
}

#[derive(Debug, Clone)]
pub struct PdfExportConfig {
    path: PathBuf,
    mode: ExportPdfMode,
}

pub struct PdfExportActor {
    render_rx: broadcast::Receiver<RenderActorRequest>,
    document: watch::Receiver<Option<Arc<TypstDocument>>>,

    config: Option<PdfExportConfig>,
}

impl PdfExportActor {
    pub fn new(
        document: watch::Receiver<Option<Arc<TypstDocument>>>,
        render_rx: broadcast::Receiver<RenderActorRequest>,
    ) -> Self {
        Self {
            render_rx,
            document,

            config: None,
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                req = self.render_rx.recv() => {
                    let req = match req {
                        Ok(req) => req,
                        Err(RecvError::Closed) => {
                            info!("render actor channel closed");
                            break;
                        }
                        Err(RecvError::Lagged(_)) => {
                            info!("render actor channel lagged");
                            continue;
                        }

                    };

                    match req {
                        RenderActorRequest::Render => {
                            let Some(document) = self.document.borrow().clone() else {
                                info!("PdfRenderActor: document is not ready");
                                continue;
                            };

                            if let Some(cfg) = self.config.as_ref() {
                                if cfg.mode == ExportPdfMode::OnType {
                                    self.export_pdf(&document, &cfg.path).await.unwrap();
                                }
                            }
                        }
                        // RenderActorRequest::ChangeConfig(config) => {
                        //     self.config = Some(config);
                        // }
                    }
                }
            }
        }
    }

    async fn export_pdf(&self, doc: &TypstDocument, path: &Path) -> anyhow::Result<()> {
        // todo: Some(pdf_uri.as_str())
        // todo: timestamp world.now()
        info!("exporting PDF {path}", path = path.display());

        let data = typst_pdf::pdf(doc, Smart::Auto, None);

        std::fs::write(path, data).context("failed to export PDF")?;

        info!("PDF export complete");

        Ok(())
    }
}
