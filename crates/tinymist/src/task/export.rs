//! The actor that handles PDF/SVG/PNG export.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::bail;
use anyhow::Context;
use log::{error, info};
use once_cell::sync::Lazy;
use tinymist_query::{ExportKind, PageSelection};
use tokio::{sync::mpsc, task::spawn_blocking};
use typst::{foundations::Smart, layout::Abs, layout::Frame, visualize::Color};
use typst_ts_compiler::EntryReader;
use typst_ts_core::{path::PathClean, ImmutPath};

use crate::{
    actor::{editor::EditorRequest, typ_server::CompiledArtifact},
    tool::word_count,
    world::LspCompilerFeat,
    ExportMode,
};

use super::*;

#[derive(Debug, Clone, Default)]
pub struct ExportConfig {
    pub substitute_pattern: String,
    pub mode: ExportMode,
}

#[derive(Debug, Clone, Copy)]
pub enum ExportSignal {
    Typed,
    Saved,
    TypedAndSaved,
    EntryChanged,
}

impl ExportSignal {
    pub fn is_typed(&self) -> bool {
        matches!(self, ExportSignal::Typed | ExportSignal::TypedAndSaved)
    }

    pub fn is_saved(&self) -> bool {
        matches!(self, ExportSignal::Saved | ExportSignal::TypedAndSaved)
    }

    fn is_entry_change(&self) -> bool {
        matches!(self, ExportSignal::EntryChanged)
    }
}

#[derive(Clone, Default)]
pub struct ExportTask {
    factory: SyncTaskFactory<ExportTaskConf>,
    export_folder: FutureFolder,
    count_word_folder: FutureFolder,
}

impl ExportTask {
    pub fn new(data: ExportTaskConf) -> Self {
        Self {
            factory: SyncTaskFactory(Arc::new(std::sync::RwLock::new(Arc::new(data)))),
            export_folder: FutureFolder::default(),
            count_word_folder: FutureFolder::default(),
        }
    }

    pub fn task(&self) -> Arc<ExportTaskConf> {
        self.factory.task()
    }

    pub fn signal(&self, snap: &CompiledArtifact<LspCompilerFeat>, s: ExportSignal) {
        let task = self.factory.task();
        task.signal(snap, s, self);
    }

    pub fn change_config(&self, config: ExportConfig) {
        self.factory.mutate(|data| data.config = config);
    }
}

#[derive(Clone, Default)]
pub struct ExportTaskConf {
    pub group: String,
    pub editor_tx: Option<mpsc::UnboundedSender<EditorRequest>>,
    pub config: ExportConfig,
    pub kind: ExportKind,
    pub count_words: bool,
}

impl ExportTaskConf {
    pub async fn oneshot(
        &self,
        snap: &CompiledArtifact<LspCompilerFeat>,
        kind: ExportKind,
    ) -> Option<PathBuf> {
        let snap = snap.clone();
        self.check_mode_and_export(&kind, &snap).await
    }

    fn signal(
        self: Arc<Self>,
        snap: &CompiledArtifact<LspCompilerFeat>,
        s: ExportSignal,
        t: &ExportTask,
    ) {
        self.signal_export(snap, s, t);
        if s.is_typed() || s.is_entry_change() {
            self.signal_count_word(snap, t);
        }
    }

    fn signal_export(
        self: &Arc<Self>,
        artifact: &CompiledArtifact<LspCompilerFeat>,
        s: ExportSignal,
        t: &ExportTask,
    ) -> Option<()> {
        let doc = artifact.doc.as_ref().ok()?;

        // We do only check the latest signal and determine whether to export by the
        // latest state. This is not a TOCTOU issue, as examined by typst-preview.
        let mode = self.config.mode;
        let need_export = (!matches!(mode, ExportMode::Never) && s.is_entry_change())
            || match mode {
                ExportMode::Never => false,
                ExportMode::OnType => s.is_typed(),
                ExportMode::OnSave => s.is_saved(),
                ExportMode::OnDocumentHasTitle => s.is_saved() && doc.title.is_some(),
            };

        if !need_export {
            return None;
        }

        let this = self.clone();
        let artifact = artifact.clone();
        t.export_folder.spawn(
            artifact.world.revision().get(),
            Box::pin(async move {
                this.check_mode_and_export(&this.kind, &artifact).await;
                Some(())
            }),
        );

        Some(())
    }

    fn signal_count_word(&self, artifact: &CompiledArtifact<LspCompilerFeat>, t: &ExportTask) {
        let Some(editor_tx) = self.editor_tx.clone() else {
            return;
        };
        if self.count_words {
            let artifact = artifact.clone();
            let group = self.group.clone();
            t.count_word_folder.spawn(
                artifact.world.revision().get(),
                Box::pin(async move {
                    let doc = artifact.doc.ok()?;

                    let wc = word_count::word_count(&doc);
                    log::debug!("word count: {wc:?}");
                    let _ = editor_tx.send(EditorRequest::WordCount(group, wc));

                    Some(())
                }),
            );
        }
    }

    async fn check_mode_and_export(
        &self,
        kind: &ExportKind,
        doc: &CompiledArtifact<LspCompilerFeat>,
    ) -> Option<PathBuf> {
        let entry = doc.world.entry_state();

        let root = entry.root();
        let main = entry.main();

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

        match self.do_export(kind, doc, &root, &path).await {
            Ok(pdf) => Some(pdf),
            Err(err) => {
                error!("RenderActor({kind:?}): failed to export {err}");
                None
            }
        }
    }

    async fn do_export(
        &self,
        kind: &ExportKind,
        doc: &CompiledArtifact<LspCompilerFeat>,
        root: &Path,
        path: &Path,
    ) -> anyhow::Result<PathBuf> {
        use ExportKind::*;
        use PageSelection::*;

        let doc = doc
            .doc
            .as_ref()
            .map_err(|_| anyhow::anyhow!("no document"))?
            .clone();

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

        let kind2 = kind.clone();
        let data = spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
            rayon::in_place_scope(|_| {
                let doc = &doc;

                static BLANK: Lazy<Frame> = Lazy::new(Frame::default);
                let first_frame = || doc.pages.first().map(|f| &f.frame).unwrap_or(&*BLANK);
                Ok(match kind2 {
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
                    Png { page: Merged } => typst_render::render_merged(
                        doc,
                        3.,
                        Color::WHITE,
                        Abs::zero(),
                        Color::WHITE,
                    )
                    .encode_png()
                    .map_err(|err| anyhow::anyhow!("failed to encode PNG ({err})"))?,
                })
            })
        });

        tokio::fs::write(&to, data.await??)
            .await
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
    fn test_default_never() {
        let conf = ExportTaskConf::default();
        assert!(!conf.count_words);
        assert_eq!(conf.config.mode, ExportMode::Never);
    }

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
