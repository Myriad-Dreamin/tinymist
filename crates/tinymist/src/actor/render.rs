//! The (PDF) render actor

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use log::{error, info};
use tokio::sync::{
    broadcast::{self, error::RecvError},
    watch,
};
use typst::foundations::Smart;
use typst_ts_core::{ImmutPath, TypstDocument};

use crate::ExportPdfMode;

#[derive(Debug, Clone)]
pub enum RenderActorRequest {
    OnTyped,
    OnSaved(PathBuf),
    ChangeExportPath(Option<ImmutPath>),
    ChangeConfig(PdfExportConfig),
}

#[derive(Debug, Clone)]
pub struct PdfExportConfig {
    pub path: Option<ImmutPath>,
    pub mode: ExportPdfMode,
}

pub struct PdfExportActor {
    render_rx: broadcast::Receiver<RenderActorRequest>,
    document: watch::Receiver<Option<Arc<TypstDocument>>>,

    pub path: Option<ImmutPath>,
    pub mode: ExportPdfMode,
}

impl PdfExportActor {
    pub fn new(
        document: watch::Receiver<Option<Arc<TypstDocument>>>,
        render_rx: broadcast::Receiver<RenderActorRequest>,
        config: Option<PdfExportConfig>,
    ) -> Self {
        Self {
            render_rx,
            document,
            path: config.as_ref().and_then(|c| c.path.clone()),
            mode: config.map(|c| c.mode).unwrap_or(ExportPdfMode::Auto),
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

                    info!("PdfRenderActor: received request: {req:?}", req = req);
                    match req {
                        RenderActorRequest::ChangeConfig(cfg) => {
                            self.path = cfg.path;
                            self.mode = cfg.mode;
                        }
                        RenderActorRequest::ChangeExportPath(cfg) => {
                            self.path = cfg;
                        }
                        _ => {
                            self.check_mode_and_export(req).await;
                        }
                    }
                }
            }
        }
    }

    async fn check_mode_and_export(&self, req: RenderActorRequest) {
        let Some(document) = self.document.borrow().clone() else {
            info!("PdfRenderActor: document is not ready");
            return;
        };

        let eq_mode = match req {
            RenderActorRequest::OnTyped => ExportPdfMode::OnType,
            RenderActorRequest::OnSaved(..) => ExportPdfMode::OnSave,
            _ => unreachable!(),
        };

        info!("PdfRenderActor: check path {:?}", self.path);
        if let Some(path) = self.path.as_ref() {
            if (get_mode(self.mode) == eq_mode) || validate_document(&req, self.mode, &document) {
                let Err(err) = self.export_pdf(&document, path).await else {
                    return;
                };
                error!("PdfRenderActor: failed to export PDF: {err}", err = err);
            }
        }

        fn get_mode(mode: ExportPdfMode) -> ExportPdfMode {
            if mode == ExportPdfMode::Auto {
                return ExportPdfMode::Never;
            }

            mode
        }

        fn validate_document(
            req: &RenderActorRequest,
            mode: ExportPdfMode,
            document: &TypstDocument,
        ) -> bool {
            info!(
                "PdfRenderActor: validating document for export mode {mode:?} title is {title}",
                title = document.title.is_some()
            );
            if mode == ExportPdfMode::OnDocumentHasTitle {
                return document.title.is_some() && matches!(req, RenderActorRequest::OnSaved(..));
            }

            false
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
