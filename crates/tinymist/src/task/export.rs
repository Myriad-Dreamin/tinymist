//! The actor that handles various document export, like PDF and SVG export.

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, OnceLock};

use reflexo::ImmutPath;
use reflexo_typst::CompilationTask;
use tinymist_project::LspWorld;
use tinymist_std::error::prelude::*;
use tinymist_std::typst::TypstDocument;
use tinymist_task::{convert_datetime, get_page_selection, ExportTarget, TextExport};
use tokio::sync::mpsc;
use typlite::Typlite;
use typst::foundations::IntoValue;
use typst::visualize::Color;
use typst_pdf::PdfOptions;

use super::{FutureFolder, SyncTaskFactory};
use crate::project::{
    convert_source_date_epoch, ApplyProjectTask, CompiledArtifact, EntryReader, ExportHtmlTask,
    ExportMarkdownTask, ExportPdfTask, ExportPngTask, ExportSvgTask,
    ExportTask as ProjectExportTask, ExportTextTask, LspCompiledArtifact, ProjectTask, QueryTask,
    TaskWhen,
};
use crate::{actor::editor::EditorRequest, tool::word_count};

#[derive(Clone)]
pub struct ExportTask {
    pub handle: tokio::runtime::Handle,
    pub editor_tx: Option<mpsc::UnboundedSender<EditorRequest>>,
    pub factory: SyncTaskFactory<ExportUserConfig>,
    export_folder: FutureFolder,
    count_word_folder: FutureFolder,
}

impl ExportTask {
    pub fn new(
        handle: tokio::runtime::Handle,
        editor_tx: Option<mpsc::UnboundedSender<EditorRequest>>,
        export_config: ExportUserConfig,
    ) -> Self {
        Self {
            handle,
            editor_tx,
            factory: SyncTaskFactory::new(export_config),
            export_folder: FutureFolder::default(),
            count_word_folder: FutureFolder::default(),
        }
    }

    pub fn change_config(&self, config: ExportUserConfig) {
        self.factory.mutate(|data| *data = config);
    }

    pub fn signal(&self, snap: &LspCompiledArtifact) {
        let config = self.factory.task();

        self.signal_export(snap, &config);
        self.signal_count_word(snap, &config);
    }

    fn signal_export(
        &self,
        artifact: &LspCompiledArtifact,
        config: &Arc<ExportUserConfig>,
    ) -> Option<()> {
        let doc = artifact.doc.as_ref().ok()?;
        let s = artifact.signal;

        let when = config.task.when().unwrap_or_default();
        let need_export = (!matches!(when, TaskWhen::Never) && s.by_entry_update)
            || match when {
                TaskWhen::Never => false,
                TaskWhen::OnType => s.by_mem_events,
                TaskWhen::OnSave => s.by_fs_events,
                TaskWhen::OnDocumentHasTitle => s.by_fs_events && doc.info().title.is_some(),
            };

        if !need_export {
            return None;
        }

        let rev = artifact.world.revision().get();
        let fut = self.export_folder.spawn(rev, || {
            let task = config.task.clone();
            let artifact = artifact.clone();
            Box::pin(async move {
                log_err(Self::do_export(task, artifact, None).await);
                Some(())
            })
        })?;

        self.handle.spawn(fut);

        Some(())
    }

    fn signal_count_word(
        &self,
        artifact: &LspCompiledArtifact,
        config: &Arc<ExportUserConfig>,
    ) -> Option<()> {
        if !config.count_words {
            return None;
        }

        let editor_tx = self.editor_tx.clone()?;
        let rev = artifact.world.revision().get();
        let fut = self.count_word_folder.spawn(rev, || {
            let artifact = artifact.clone();
            Box::pin(async move {
                let id = artifact.snap.id;
                let doc = artifact.doc.ok()?;
                let wc =
                    log_err(FutureFolder::compute(move |_| word_count::word_count(&doc)).await);
                log::debug!("WordCount({id:?}:{rev}): {wc:?}");

                if let Some(wc) = wc {
                    let _ = editor_tx.send(EditorRequest::WordCount(id, wc));
                }

                Some(())
            })
        })?;

        self.handle.spawn(fut);

        Some(())
    }

    pub async fn do_export(
        task: ProjectTask,
        artifact: LspCompiledArtifact,
        lock_dir: Option<ImmutPath>,
    ) -> Result<Option<PathBuf>> {
        use reflexo_vec2svg::DefaultExportFeature;
        use ProjectTask::*;

        let CompiledArtifact { snap, doc, .. } = artifact;

        // Prepare the output path.
        let entry = snap.world.entry_state();
        let config = task.as_export().unwrap();
        let output = config.output.clone().unwrap_or_default();
        let Some(to) = output.substitute(&entry) else {
            return Ok(None);
        };
        if to.is_relative() {
            bail!("ExportTask({task:?}): output path is relative: {to:?}");
        }
        if to.is_dir() {
            bail!("ExportTask({task:?}): output path is a directory: {to:?}");
        }
        let to = to.with_extension(task.extension());

        static EXPORT_ID: AtomicUsize = AtomicUsize::new(0);
        let export_id = EXPORT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        log::info!("ExportTask({export_id}): exporting {entry:?} to {to:?}");
        if let Some(e) = to.parent() {
            if !e.exists() {
                std::fs::create_dir_all(e).context("failed to create directory")?;
            }
        }

        let _: Option<()> = lock_dir.and_then(|lock_dir| {
            let mut updater = crate::project::update_lock(lock_dir);

            let doc_id = updater.compiled(&snap.world)?;

            updater.task(ApplyProjectTask {
                id: doc_id.clone(),
                document: doc_id,
                task: task.clone(),
            });
            updater.commit();

            Some(())
        });

        // Prepare the document.
        let doc = doc.ok().context("cannot export with compilation errors")?;

        // Prepare data.
        let kind2 = task.clone();
        let data = FutureFolder::compute(move |_| -> Result<Vec<u8>> {
            let doc = &doc;

            // static BLANK: Lazy<Page> = Lazy::new(Page::default);
            // todo: check warnings and errors inside
            let html_once = OnceLock::new();
            let html_doc = || -> Result<_> {
                html_once
                    .get_or_init(|| -> Result<_> {
                        Ok(match &doc {
                            TypstDocument::Html(html_doc) => html_doc.clone(),
                            TypstDocument::Paged(_) => extra_compile_for_export(&snap.world)?,
                        })
                    })
                    .as_ref()
                    .map_err(|e| e.clone())
            };
            let page_once = OnceLock::new();
            let paged_doc = || {
                page_once
                    .get_or_init(|| -> Result<_> {
                        Ok(match &doc {
                            TypstDocument::Paged(paged_doc) => paged_doc.clone(),
                            TypstDocument::Html(_) => extra_compile_for_export(&snap.world)?,
                        })
                    })
                    .as_ref()
                    .map_err(|e| e.clone())
            };
            let first_page = || {
                paged_doc()?
                    .pages
                    .first()
                    .context("no first page to export")
            };
            Ok(match kind2 {
                Preview(..) => vec![],
                // todo: more pdf flags
                ExportPdf(ExportPdfTask {
                    creation_timestamp, ..
                }) => {
                    // todo: timestamp world.now()
                    let creation_timestamp = creation_timestamp
                        .map(convert_source_date_epoch)
                        .transpose()
                        .context_ut("parse pdf creation timestamp")?
                        .unwrap_or_else(chrono::Utc::now);

                    // todo: Some(pdf_uri.as_str())
                    typst_pdf::pdf(
                        paged_doc()?,
                        &PdfOptions {
                            timestamp: convert_datetime(creation_timestamp),
                            ..PdfOptions::default()
                        },
                    )
                    .map_err(|e| anyhow::anyhow!("failed to convert to pdf: {e:?}"))?
                }
                Query(QueryTask {
                    export: _,
                    output_extension: _,
                    format,
                    selector,
                    field,
                    one,
                }) => {
                    let pretty = false;
                    let elements = reflexo_typst::query::retrieve(&snap.world, &selector, doc)
                        .map_err(|e| anyhow::anyhow!("failed to retrieve: {e}"))?;
                    if one && elements.len() != 1 {
                        bail!("expected exactly one element, found {}", elements.len());
                    }

                    let mapped: Vec<_> = elements
                        .into_iter()
                        .filter_map(|c| match &field {
                            Some(field) => c.get_by_name(field).ok(),
                            _ => Some(c.into_value()),
                        })
                        .collect();

                    if one {
                        let Some(value) = mapped.first() else {
                            bail!("no such field found for element");
                        };
                        serialize(value, &format, pretty).map(String::into_bytes)?
                    } else {
                        serialize(&mapped, &format, pretty).map(String::into_bytes)?
                    }
                }
                ExportHtml(ExportHtmlTask { export: _ }) => typst_html::html(html_doc()?)
                    .map_err(|e| format!("export error: {e:?}"))
                    .context_ut("failed to export to html")?
                    .into_bytes(),
                ExportSvgHtml(ExportHtmlTask { export: _ }) => {
                    reflexo_vec2svg::render_svg_html::<DefaultExportFeature>(paged_doc()?)
                        .into_bytes()
                }
                ExportText(ExportTextTask { export: _ }) => {
                    TextExport::run_on_doc(doc)?.into_bytes()
                }
                ExportMd(ExportMarkdownTask { export: _ }) => {
                    let conv = Typlite::new(Arc::new(snap.world))
                        .convert()
                        .map_err(|e| anyhow::anyhow!("failed to convert to markdown: {e}"))?;

                    conv.as_bytes().to_owned()
                }
                ExportSvg(ExportSvgTask { export }) => {
                    let (is_first, merged_gap) = get_page_selection(&export)?;

                    if is_first {
                        typst_svg::svg(first_page()?).into_bytes()
                    } else {
                        typst_svg::svg_merged(paged_doc()?, merged_gap).into_bytes()
                    }
                }
                ExportPng(ExportPngTask { export, ppi, fill }) => {
                    let ppi = ppi.to_f32();
                    if ppi <= 1e-6 {
                        bail!("invalid ppi: {ppi}");
                    }

                    let fill = if let Some(fill) = fill {
                        parse_color(fill).map_err(|err| anyhow::anyhow!("invalid fill ({err})"))?
                    } else {
                        Color::WHITE
                    };

                    let (is_first, merged_gap) = get_page_selection(&export)?;

                    let pixmap = if is_first {
                        typst_render::render(first_page()?, ppi / 72.)
                    } else {
                        typst_render::render_merged(paged_doc()?, ppi / 72., merged_gap, Some(fill))
                    };

                    pixmap
                        .encode_png()
                        .map_err(|err| anyhow::anyhow!("failed to encode PNG ({err})"))?
                }
            })
        });

        tokio::fs::write(&to, data.await??)
            .await
            .context("failed to export")?;

        log::info!("ExportTask({export_id}): export complete");
        Ok(Some(to))
    }
}

/// User configuration for export.
#[derive(Clone, PartialEq, Eq)]
pub struct ExportUserConfig {
    /// Tinymist's default export target.
    pub export_target: ExportTarget,
    pub task: ProjectTask,
    pub count_words: bool,
}

impl Default for ExportUserConfig {
    fn default() -> Self {
        Self {
            export_target: ExportTarget::default(),
            task: ProjectTask::ExportPdf(ExportPdfTask {
                export: ProjectExportTask {
                    when: TaskWhen::Never,
                    output: None,
                    transform: vec![],
                },
                pdf_standards: vec![],
                creation_timestamp: None,
            }),
            count_words: false,
        }
    }
}

fn parse_color(fill: String) -> Result<Color> {
    match fill.as_str() {
        "black" => Ok(Color::BLACK),
        "white" => Ok(Color::WHITE),
        "red" => Ok(Color::RED),
        "green" => Ok(Color::GREEN),
        "blue" => Ok(Color::BLUE),
        hex if hex.starts_with('#') => {
            Color::from_str(&hex[1..]).context_ut("failed to parse color")
        }
        _ => bail!("invalid color: {fill}"),
    }
}

fn log_err<T>(artifact: Result<T>) -> Option<T> {
    match artifact {
        Ok(v) => Some(v),
        Err(err) => {
            log::error!("{err}");
            None
        }
    }
}

fn extra_compile_for_export<D: typst::Document + Send + Sync + 'static>(
    world: &LspWorld,
) -> Result<Arc<D>> {
    let res = tokio::task::block_in_place(|| CompilationTask::<D>::execute(world));

    match res.output {
        Ok(v) => Ok(v),
        Err(e) if e.is_empty() => bail!("failed to compile: internal error"),
        Err(e) => bail!("failed to compile: {}", e[0].message),
    }
}

/// Serialize data to the output format.
fn serialize(data: &impl serde::Serialize, format: &str, pretty: bool) -> Result<String> {
    Ok(match format {
        "json" if pretty => serde_json::to_string_pretty(data).context("serialize to json")?,
        "json" => serde_json::to_string(data).context("serialize to json")?,
        "yaml" => serde_yaml::to_string(&data).context_ut("serialize to yaml")?,
        "txt" => {
            use serde_json::Value::*;
            let value = serde_json::to_value(data).context("serialize to json value")?;
            match value {
                String(s) => s,
                _ => {
                    let kind = match value {
                        Null => "null",
                        Bool(_) => "boolean",
                        Number(_) => "number",
                        String(_) => "string",
                        Array(_) => "array",
                        Object(_) => "object",
                    };
                    bail!("expected a string value for format: {format}, got {kind}")
                }
            }
        }
        _ => bail!("unsupported format for query: {format}"),
    })
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::export::ProjectCompilation;
    use crate::project::{CompileOnceArgs, ExportSignal};
    use crate::world::base::{CompileSnapshot, WorldComputeGraph};

    #[test]
    fn test_default_never() {
        let conf = ExportUserConfig::default();
        assert!(!conf.count_words);
        assert_eq!(conf.task.when(), Some(TaskWhen::Never));
    }

    #[test]
    fn test_parse_color() {
        assert_eq!(parse_color("black".to_owned()).unwrap(), Color::BLACK);
        assert_eq!(parse_color("white".to_owned()).unwrap(), Color::WHITE);
        assert_eq!(parse_color("red".to_owned()).unwrap(), Color::RED);
        assert_eq!(parse_color("green".to_owned()).unwrap(), Color::GREEN);
        assert_eq!(parse_color("blue".to_owned()).unwrap(), Color::BLUE);
        assert_eq!(
            parse_color("#000000".to_owned()).unwrap().to_hex(),
            "#000000"
        );
        assert_eq!(
            parse_color("#ffffff".to_owned()).unwrap().to_hex(),
            "#ffffff"
        );
        assert_eq!(
            parse_color("#000000cc".to_owned()).unwrap().to_hex(),
            "#000000cc"
        );
        assert!(parse_color("invalid".to_owned()).is_err());
    }

    #[test]
    fn compilation_default_never() {
        let args = CompileOnceArgs::parse_from(["tinymist", "main.typ"]);
        let verse = args
            .resolve_system()
            .expect("failed to resolve system universe");

        let snap = CompileSnapshot::from_world(verse.snapshot());

        let graph = WorldComputeGraph::new(snap);

        let needs_run =
            ProjectCompilation::preconfig_timings(&graph).expect("failed to preconfigure timings");

        assert!(!needs_run);
    }

    // todo: on demand compilation
    #[test]
    fn compilation_run_paged_diagnostics() {
        let args = CompileOnceArgs::parse_from(["tinymist", "main.typ"]);
        let verse = args
            .resolve_system()
            .expect("failed to resolve system universe");

        let mut snap = CompileSnapshot::from_world(verse.snapshot());

        snap.signal = ExportSignal {
            by_entry_update: true,
            by_fs_events: false,
            by_mem_events: false,
        };

        let graph = WorldComputeGraph::new(snap);

        let needs_run =
            ProjectCompilation::preconfig_timings(&graph).expect("failed to preconfigure timings");

        assert!(needs_run);
    }
}
