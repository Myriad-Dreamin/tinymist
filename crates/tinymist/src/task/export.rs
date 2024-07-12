//! The actor that handles various document export, like PDF and SVG export.

use std::{path::PathBuf, sync::Arc};

use anyhow::{bail, Context};
use once_cell::sync::Lazy;
use tinymist_query::{ExportKind, PageSelection};
use tokio::{sync::mpsc, task::spawn_blocking};
use typst::{foundations::Smart, layout::Abs, layout::Frame, visualize::Color};
use typst_ts_compiler::{EntryReader, EntryState, TaskInputs};

use crate::{
    actor::{
        editor::EditorRequest,
        typ_client::QuerySnap,
        typ_server::{CompiledArtifact, ExportSignal},
    },
    tool::word_count,
    world::LspCompilerFeat,
    ExportMode, PathPattern,
};

use super::*;

/// User configuration for export.
#[derive(Debug, Clone, Default)]
pub struct ExportUserConfig {
    /// The output path pattern.
    pub output: PathPattern,
    /// The export mode.
    pub mode: ExportMode,
}

#[derive(Clone, Default)]
pub struct ExportTask {
    factory: SyncTaskFactory<ExportConfig>,
    export_folder: FutureFolder,
    count_word_folder: FutureFolder,
}

impl ExportTask {
    pub fn new(data: ExportConfig) -> Self {
        Self {
            factory: SyncTaskFactory::new(data),
            ..ExportTask::default()
        }
    }

    pub fn change_config(&self, config: ExportUserConfig) {
        self.factory.mutate(|data| data.config = config);
    }

    pub fn signal(&self, snap: &CompiledArtifact<LspCompilerFeat>, s: ExportSignal) {
        let task = self.factory.task();
        task.signal(snap, s, self);
    }

    pub fn oneshot(
        &self,
        snap: QuerySnap,
        entry: Option<EntryState>,
        kind: ExportKind,
    ) -> impl Future<Output = anyhow::Result<Option<PathBuf>>> {
        let export = self.factory.task();
        async move {
            let snap = snap.snapshot().await?;
            let snap = snap.task(TaskInputs {
                entry,
                ..Default::default()
            });

            let artifact = snap.compile().await;
            export.do_export(&kind, &artifact).await
        }
    }
}

#[derive(Clone, Default)]
pub struct ExportConfig {
    pub group: String,
    pub editor_tx: Option<mpsc::UnboundedSender<EditorRequest>>,
    pub config: ExportUserConfig,
    pub kind: ExportKind,
    pub count_words: bool,
}

impl ExportConfig {
    fn signal(
        self: Arc<Self>,
        snap: &CompiledArtifact<LspCompilerFeat>,
        s: ExportSignal,
        t: &ExportTask,
    ) {
        self.signal_export(snap, s, t);
        self.signal_count_word(snap, t);
    }

    fn signal_export(
        self: &Arc<Self>,
        artifact: &CompiledArtifact<LspCompilerFeat>,
        s: ExportSignal,
        t: &ExportTask,
    ) -> Option<()> {
        let doc = artifact.doc.as_ref().ok()?;

        let mode = self.config.mode;
        let need_export = (!matches!(mode, ExportMode::Never) && s.by_entry_update)
            || match mode {
                ExportMode::Never => false,
                ExportMode::OnType => s.by_mem_events,
                ExportMode::OnSave => s.by_fs_events,
                ExportMode::OnDocumentHasTitle => s.by_fs_events && doc.title.is_some(),
            };

        if !need_export {
            return None;
        }

        t.export_folder.spawn(artifact.world.revision().get(), || {
            let this = self.clone();
            let artifact = artifact.clone();
            Box::pin(async move {
                log_err(this.do_export(&this.kind, &artifact).await);
                Some(())
            })
        });

        Some(())
    }

    fn signal_count_word(&self, artifact: &CompiledArtifact<LspCompilerFeat>, t: &ExportTask) {
        let Some(editor_tx) = self.editor_tx.clone() else {
            return;
        };
        if self.count_words {
            let revision = artifact.world.revision().get();
            t.count_word_folder.spawn(revision, || {
                let artifact = artifact.clone();
                let group = self.group.clone();
                Box::pin(async move {
                    let doc = artifact.doc.ok()?;
                    let wc = word_count::word_count(&doc);
                    log::debug!("WordCount({group}:{revision}): {wc:?}");

                    let _ = editor_tx.send(EditorRequest::WordCount(group, wc));

                    Some(())
                })
            });
        }
    }

    async fn do_export(
        &self,
        kind: &ExportKind,
        doc: &CompiledArtifact<LspCompilerFeat>,
    ) -> anyhow::Result<Option<PathBuf>> {
        use ExportKind::*;
        use PageSelection::*;

        let entry = doc.world.entry_state();

        let doc = doc
            .doc
            .as_ref()
            .map_err(|_| anyhow::anyhow!("no document"))?
            .clone();

        let Some(to) = self.config.output.substitute(&entry) else {
            return Ok(None);
        };
        if to.is_relative() {
            bail!("RenderActor({kind:?}): path is relative: {to:?}");
        }
        if to.is_dir() {
            bail!("RenderActor({kind:?}): path is a directory: {to:?}");
        }

        let to = to.with_extension(kind.extension());
        log::info!("RenderActor({kind:?}): exporting {entry:?} to {to:?}");

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

        log::info!("RenderActor({kind:?}): export complete");
        Ok(Some(to))
    }
}

fn log_err<T>(artifact: anyhow::Result<T>) -> Option<T> {
    match artifact {
        Ok(v) => Some(v),
        Err(err) => {
            log::error!("{err}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_never() {
        let conf = ExportConfig::default();
        assert!(!conf.count_words);
        assert_eq!(conf.config.mode, ExportMode::Never);
    }
}
