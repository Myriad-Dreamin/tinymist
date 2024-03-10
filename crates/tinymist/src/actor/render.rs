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
use typst_ts_core::{path::PathClean, ImmutPath, TypstDocument};

use crate::ExportPdfMode;

#[derive(Debug, Clone)]
pub enum RenderActorRequest {
    OnTyped,
    OnSaved(PathBuf),
    ChangeExportPath(PdfPathVars),
    ChangeConfig(PdfExportConfig),
}

#[derive(Debug, Clone)]
pub struct PdfPathVars {
    pub root: ImmutPath,
    pub path: Option<ImmutPath>,
}

#[derive(Debug, Clone)]
pub struct PdfExportConfig {
    pub substitute_pattern: String,
    pub root: ImmutPath,
    pub path: Option<ImmutPath>,
    pub mode: ExportPdfMode,
}

pub struct PdfExportActor {
    render_rx: broadcast::Receiver<RenderActorRequest>,
    document: watch::Receiver<Option<Arc<TypstDocument>>>,

    pub substitute_pattern: String,
    pub root: ImmutPath,
    pub path: Option<ImmutPath>,
    pub mode: ExportPdfMode,
}

impl PdfExportActor {
    pub fn new(
        document: watch::Receiver<Option<Arc<TypstDocument>>>,
        render_rx: broadcast::Receiver<RenderActorRequest>,
        config: PdfExportConfig,
    ) -> Self {
        Self {
            render_rx,
            document,
            substitute_pattern: config.substitute_pattern,
            root: config.root,
            path: config.path,
            mode: config.mode,
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
                            self.substitute_pattern = cfg.substitute_pattern;
                            self.root = cfg.root;
                            self.path = cfg.path;
                            self.mode = cfg.mode;
                        }
                        RenderActorRequest::ChangeExportPath(cfg) => {
                            self.root = cfg.root;
                            self.path = cfg.path;
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

        info!(
            "PdfRenderActor: check path {:?} with output directory {}",
            self.path, self.substitute_pattern
        );
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
        let Some(to) = substitute_path(&self.substitute_pattern, &self.root, path) else {
            return Err(anyhow::anyhow!("failed to substitute path"));
        };
        if to.is_relative() {
            return Err(anyhow::anyhow!("path is relative: {to:?}"));
        }
        if to.is_dir() {
            return Err(anyhow::anyhow!("path is a directory: {to:?}"));
        }

        let to = to.with_extension("pdf");
        info!("exporting PDF {path:?} to {to:?}");

        if let Some(e) = to.parent() {
            if !e.exists() {
                std::fs::create_dir_all(e).context("failed to create directory")?;
            }
        }

        // todo: Some(pdf_uri.as_str())
        // todo: timestamp world.now()
        let data = typst_pdf::pdf(doc, Smart::Auto, None);

        std::fs::write(to, data).context("failed to export PDF")?;

        info!("PDF export complete");
        Ok(())
    }
}

#[comemo::memoize]
fn substitute_path(substitute_pattern: &str, root: &Path, path: &Path) -> Option<ImmutPath> {
    if substitute_pattern.is_empty() {
        return Some(path.to_path_buf().clean().into());
    }

    let path = path.strip_prefix(root).ok()?;
    let dir = path.parent();
    let file_name = path.file_name().unwrap_or_default();

    let w = root.to_string_lossy();
    let f = file_name.to_string_lossy();

    // replace all $root
    let mut path = substitute_pattern.replace("$root", &w);
    if let Some(dir) = dir {
        let d = dir.to_string_lossy();
        path = path.replace("$dir", &d);
    }
    path = path.replace("$name", &f);

    Some(PathBuf::from(path).clean().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_substitute_path() {
        let root = Path::new("/root");
        let path = Path::new("/root/dir1/dir2/file.txt");

        assert_eq!(
            substitute_path("/substitute/$dir/$name", root, path),
            Some(PathBuf::from("/substitute/dir1/dir2/file.txt").into())
        );
        assert_eq!(
            substitute_path("/substitute/$dir/../$name", root, path),
            Some(PathBuf::from("/substitute/dir1/file.txt").into())
        );
        assert_eq!(
            substitute_path("/substitute/$name", root, path),
            Some(PathBuf::from("/substitute/file.txt").into())
        );
        assert_eq!(
            substitute_path("/substitute/target/$dir/$name", root, path),
            Some(PathBuf::from("/substitute/target/dir1/dir2/file.txt").into())
        );
    }
}
