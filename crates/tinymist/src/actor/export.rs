//! The actor that handles PDF export.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::bail;
use anyhow::Context;
use log::{error, info};
use once_cell::sync::Lazy;
use tinymist_query::{ExportKind, PageSelection};
use tokio::sync::{mpsc, oneshot, watch};
use typst::{foundations::Smart, layout::Abs, layout::Frame, visualize::Color};
use typst_ts_core::{config::compiler::EntryState, path::PathClean, ImmutPath, TypstDocument};

use crate::{tools::word_count, ExportMode};

use super::editor::EditorRequest;

#[derive(Debug, Clone, Default)]
pub struct ExportConfig {
    pub substitute_pattern: String,
    pub entry: EntryState,
    pub mode: ExportMode,
}

#[derive(Debug)]
pub enum ExportRequest {
    OnTyped,
    OnSaved(PathBuf),
    Oneshot(Option<ExportKind>, oneshot::Sender<Option<PathBuf>>),
    ChangeConfig(ExportConfig),
    ChangeExportPath(EntryState),
}

pub struct ExportActor {
    group: String,
    editor_tx: mpsc::UnboundedSender<EditorRequest>,
    export_rx: mpsc::UnboundedReceiver<ExportRequest>,
    document: watch::Receiver<Option<Arc<TypstDocument>>>,

    pub config: ExportConfig,
    pub kind: ExportKind,
    pub count_words: bool,
}

impl ExportActor {
    pub fn new(
        group: String,
        document: watch::Receiver<Option<Arc<TypstDocument>>>,
        editor_tx: mpsc::UnboundedSender<EditorRequest>,
        export_rx: mpsc::UnboundedReceiver<ExportRequest>,
        config: ExportConfig,
        kind: ExportKind,
        count_words: bool,
    ) -> Self {
        Self {
            group,
            editor_tx,
            export_rx,
            document,
            config,
            kind,
            count_words,
        }
    }

    pub async fn run(mut self) {
        // todo: accumulate like compile server if we have performance issue here
        let kind = &self.kind;
        while let Some(req) = self.export_rx.recv().await {
            log::debug!("RenderActor: received request: {req:?}");
            match req {
                ExportRequest::ChangeConfig(cfg) => self.config = cfg,
                ExportRequest::ChangeExportPath(entry) => self.config.entry = entry,
                ExportRequest::OnTyped => {
                    let export = self.config.mode == ExportMode::OnType;
                    self.check_mode_and_export(&self.kind, export).await;
                }
                ExportRequest::OnSaved(..) => {
                    let export = match self.config.mode {
                        ExportMode::OnSave => true,
                        ExportMode::OnDocumentHasTitle => self.doc_has_title(),
                        _ => false,
                    };
                    self.check_mode_and_export(&self.kind, export).await;
                }
                ExportRequest::Oneshot(kind, callback) => {
                    let kind = kind.as_ref().unwrap_or(&self.kind);
                    let resp = self.check_mode_and_export(kind, true).await;
                    if let Err(err) = callback.send(resp) {
                        error!("RenderActor(@{kind:?}): failed to send response: {err:?}");
                    }
                }
            }
        }
        info!("RenderActor(@{kind:?}): stopped");
    }

    fn doc_has_title(&self) -> bool {
        let doc = self.document.borrow();
        doc.as_ref().and_then(|d| d.title.as_ref()).is_some()
    }

    async fn check_mode_and_export(&self, kind: &ExportKind, export: bool) -> Option<PathBuf> {
        let Some(document) = self.document.borrow().clone() else {
            info!("RenderActor: document is not ready");
            return None;
        };

        // pub entry: EntryState,
        let root = self.config.entry.root();
        let main = self.config.entry.main();

        info!(
            "RenderActor: check path {:?} and root {:?} with output directory {}",
            main, root, self.config.substitute_pattern
        );

        let root = root?;
        let main = main?;

        // todo: package??
        if main.package().is_some() {
            return None;
        }

        let path = main.vpath().resolve(&root)?;

        // Count words if needed.
        if self.count_words {
            let wc = word_count::word_count(&document);
            log::debug!("word count: {wc:?}");
            let _ = self
                .editor_tx
                .send(EditorRequest::WordCount(self.group.clone(), Some(wc)));
        }

        // Export if needed.
        if export {
            return match self.export(kind, &document, &root, &path).await {
                Ok(pdf) => Some(pdf),
                Err(err) => {
                    error!("RenderActor({kind:?}): failed to export {err}");
                    None
                }
            };
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
        use ExportKind::*;
        use PageSelection::*;

        let Some(to) = substitute_path(&self.config.substitute_pattern, root, path) else {
            bail!("RenderActor({kind:?}): failed to substitute path");
        };
        if to.is_relative() {
            bail!("RenderActor({kind:?}): path is relative: {to:?}");
        }
        if to.is_dir() {
            bail!("RenderActor({kind:?}): path is a directory: {to:?}");
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

        static BLANK: Lazy<Frame> = Lazy::new(Frame::default);
        let first_frame = || doc.pages.first().map(|f| &f.frame).unwrap_or(&*BLANK);
        let data = match kind {
            Pdf => {
                // todo: Some(pdf_uri.as_str())
                // todo: timestamp world.now()
                typst_pdf::pdf(doc, Smart::Auto, None)
            }
            Svg { page: First } => typst_svg::svg(first_frame()).into_bytes(),
            Svg { page: Merged } => typst_svg::svg_merged(doc, Abs::zero()).into_bytes(),
            Png { page: First } => typst_render::render(first_frame(), 3., Color::WHITE)
                .encode_png()
                .map_err(|err| anyhow::anyhow!("failed to encode PNG ({err})"))?,
            Png { page: Merged } => {
                typst_render::render_merged(doc, 3., Color::WHITE, Abs::zero(), Color::WHITE)
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
