//! The (PDF) render actor

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use log::{error, info};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tinymist_query::{ExportKind, PageSelection};
use tokio::sync::{
    broadcast::{self, error::RecvError},
    oneshot, watch,
};
use typst::{foundations::Smart, layout::Frame};
use typst_ts_core::{config::compiler::EntryState, path::PathClean, ImmutPath, TypstDocument};

use crate::ExportMode;

#[derive(Debug, Clone)]
pub struct OneshotRendering {
    pub kind: Option<ExportKind>,
    // todo: bad arch...
    pub callback: Arc<Mutex<Option<oneshot::Sender<Option<PathBuf>>>>>,
}

#[derive(Debug, Clone)]
pub enum RenderActorRequest {
    OnTyped,
    Oneshot(OneshotRendering),
    OnSaved(PathBuf),
    ChangeExportPath(PathVars),
    ChangeConfig(ExportConfig),
}

#[derive(Debug, Clone)]
pub struct PathVars {
    pub entry: EntryState,
}

#[derive(Debug, Clone, Default)]
pub struct ExportConfig {
    pub substitute_pattern: String,
    pub entry: EntryState,
    pub mode: ExportMode,
}

pub struct ExportActor {
    render_rx: broadcast::Receiver<RenderActorRequest>,
    document: watch::Receiver<Option<Arc<TypstDocument>>>,

    pub substitute_pattern: String,
    pub entry: EntryState,
    pub mode: ExportMode,
    pub kind: ExportKind,
}

impl ExportActor {
    pub fn new(
        document: watch::Receiver<Option<Arc<TypstDocument>>>,
        render_rx: broadcast::Receiver<RenderActorRequest>,
        config: ExportConfig,
    ) -> Self {
        Self {
            render_rx,
            document,
            substitute_pattern: config.substitute_pattern,
            entry: config.entry,
            mode: config.mode,
            kind: ExportKind::Pdf,
        }
    }

    pub async fn run(mut self) {
        let kind = &self.kind;
        loop {
            tokio::select! {
                req = self.render_rx.recv() => {
                    let req = match req {
                        Ok(req) => req,
                        Err(RecvError::Closed) => {
                            info!("RenderActor(@{kind:?}): channel closed");
                            break;
                        }
                        Err(RecvError::Lagged(_)) => {
                            info!("RenderActor(@{kind:?}): channel lagged");
                            continue;
                        }

                    };

                    info!("RenderActor: received request: {req:?}", req = req);
                    match req {
                        RenderActorRequest::ChangeConfig(cfg) => {
                            self.substitute_pattern = cfg.substitute_pattern;
                            self.entry = cfg.entry;
                            self.mode = cfg.mode;
                        }
                        RenderActorRequest::ChangeExportPath(cfg) => {
                            self.entry = cfg.entry;
                        }
                        _ => {
                            let cb = match &req {
                                RenderActorRequest::Oneshot(oneshot) => Some(oneshot.callback.clone()),
                                _ => None,
                            };
                            let resp = self.check_mode_and_export(req).await;
                            if let Some(cb) = cb {
                                let Some(cb) = cb.lock().take() else {
                                    error!("RenderActor(@{kind:?}): oneshot.callback is None");
                                    continue;
                                };
                                if let Err(e) = cb.send(resp) {
                                    error!("RenderActor(@{kind:?}): failed to send response: {err:?}", err = e);
                                }
                            }
                        }
                    }
                }
            }
        }
        info!("RenderActor(@{kind:?}): stopped");
    }

    async fn check_mode_and_export(&self, req: RenderActorRequest) -> Option<PathBuf> {
        let Some(document) = self.document.borrow().clone() else {
            info!("RenderActor: document is not ready");
            return None;
        };

        let eq_mode = match req {
            RenderActorRequest::OnTyped => ExportMode::OnType,
            RenderActorRequest::Oneshot(..) => ExportMode::OnSave,
            RenderActorRequest::OnSaved(..) => ExportMode::OnSave,
            _ => unreachable!(),
        };

        let kind = if let RenderActorRequest::Oneshot(oneshot) = &req {
            oneshot.kind.as_ref()
        } else {
            None
        };
        let kind = kind.unwrap_or(&self.kind);

        // pub entry: EntryState,
        let root = self.entry.root();
        let main = self.entry.main();

        info!(
            "RenderActor: check path {:?} and root {:?} with output directory {}",
            main, root, self.substitute_pattern
        );

        let root = root?;
        let main = main?;

        // todo: package??
        if main.package().is_some() {
            return None;
        }

        let path = main.vpath().resolve(&root)?;

        let should_do = matches!(req, RenderActorRequest::Oneshot(..));
        let should_do = should_do || get_mode(self.mode) == eq_mode;
        let should_do = should_do
            || 'validate_doc: {
                let mode = self.mode;
                info!(
                    "RenderActor: validating document for export mode {mode:?} title is {title}",
                    title = document.title.is_some()
                );
                if mode == ExportMode::OnDocumentHasTitle {
                    break 'validate_doc document.title.is_some()
                        && matches!(req, RenderActorRequest::OnSaved(..));
                }

                false
            };
        if should_do {
            return match self.export(kind, &document, &root, &path).await {
                Ok(pdf) => Some(pdf),
                Err(err) => {
                    error!("RenderActor({kind:?}): failed to export {err}", err = err);
                    None
                }
            };
        }

        fn get_mode(mode: ExportMode) -> ExportMode {
            if mode == ExportMode::Auto {
                return ExportMode::Never;
            }

            mode
        }

        None
    }

    async fn export(
        &self,
        kind: &ExportKind,
        doc: &TypstDocument,
        root: &Path,
        path: &Path,
    ) -> anyhow::Result<PathBuf> {
        let Some(to) = substitute_path(&self.substitute_pattern, root, path) else {
            return Err(anyhow::anyhow!(
                "RenderActor({kind:?}): failed to substitute path"
            ));
        };
        if to.is_relative() {
            return Err(anyhow::anyhow!(
                "RenderActor({kind:?}): path is relative: {to:?}"
            ));
        }
        if to.is_dir() {
            return Err(anyhow::anyhow!(
                "RenderActor({kind:?}): path is a directory: {to:?}"
            ));
        }

        let to = to.with_extension(kind.extension());
        info!("RenderActor({kind:?}): exporting {path:?} to {to:?}");

        if let Some(e) = to.parent() {
            if !e.exists() {
                std::fs::create_dir_all(e).with_context(|| {
                    format!("RenderActor({kind:?}): failed to create directory")
                })?;
            }
        }

        static DEFAULT_FRAME: Lazy<Frame> = Lazy::new(Frame::default);
        let data = match kind {
            ExportKind::Pdf => {
                // todo: Some(pdf_uri.as_str())
                // todo: timestamp world.now()
                typst_pdf::pdf(doc, Smart::Auto, None)
            }
            ExportKind::Svg {
                page: PageSelection::First,
            } => typst_svg::svg(
                doc.pages
                    .first()
                    .map(|f| &f.frame)
                    .unwrap_or(&*DEFAULT_FRAME),
            )
            .into_bytes(),
            ExportKind::Svg {
                page: PageSelection::Merged,
            } => typst_svg::svg_merged(doc, typst::layout::Abs::zero()).into_bytes(),
            ExportKind::Png {
                page: PageSelection::First,
            } => {
                let pixmap = typst_render::render(
                    doc.pages
                        .first()
                        .map(|f| &f.frame)
                        .unwrap_or(&*DEFAULT_FRAME),
                    3.,
                    typst::visualize::Color::WHITE,
                );
                pixmap
                    .encode_png()
                    .map_err(|err| anyhow::anyhow!("failed to encode PNG ({err})"))?
            }
            ExportKind::Png {
                page: PageSelection::Merged,
            } => {
                let pixmap = typst_render::render_merged(
                    doc,
                    3.,
                    typst::visualize::Color::WHITE,
                    typst::layout::Abs::zero(),
                    typst::visualize::Color::WHITE,
                );
                pixmap
                    .encode_png()
                    .map_err(|err| anyhow::anyhow!("failed to encode PNG ({err})"))?
            }
        };

        std::fs::write(&to, data)
            .with_context(|| format!("RenderActor({kind:?}): failed to export"))?;

        info!("RenderActor({kind:?}): export complete");
        Ok(to)
    }
}

#[comemo::memoize]
fn substitute_path(substitute_pattern: &str, root: &Path, path: &Path) -> Option<ImmutPath> {
    if let Ok(path) = path.strip_prefix("/untitled") {
        let tmp = std::env::temp_dir();
        let path = tmp.join("typst").join(path);
        return Some(path.as_path().into());
    }

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
