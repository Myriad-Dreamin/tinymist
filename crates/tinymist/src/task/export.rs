//! The actor that handles various document export, like PDF and SVG export.

use std::str::FromStr;
use std::{path::PathBuf, sync::Arc};

use crate::project::{
    CompiledArtifact, ExportHtmlTask, ExportMarkdownTask, ExportPdfTask, ExportPngTask,
    ExportTextTask, ApplyProjectTask, TaskWhen,
};
use anyhow::{bail, Context};
use chrono::DateTime;
use reflexo::ImmutPath;
use reflexo_typst::TypstDatetime;
use tinymist_project::{
    EntryReader, ExportTask as ProjectExportTask, LspCompileSnapshot, LspCompiledArtifact,
    PathPattern, ProjectTask, QueryTask,
};
use tokio::sync::mpsc;
use typlite::Typlite;
use typst::foundations::IntoValue;
use typst::{
    layout::Abs,
    syntax::{ast, SyntaxNode},
    visualize::Color,
};
use typst_pdf::PdfOptions;

use crate::tool::text::FullTextDigest;
use crate::{actor::editor::EditorRequest, tool::word_count};

use super::*;

// let when = self.config.when;

// // todo: page transforms
// let transforms = vec![];

// use tinymist_project::ExportTask as ProjectExportTask;

// let export = ProjectExportTask {
//     when,
//     transform: transforms,
// };

// let config = match kind {
//     Pdf { creation_timestamp } => {
//         let _ = creation_timestamp;
//         ProjectTaskConfig::ExportPdf(ExportPdfTask {
//             export,
//             pdf_standards: Default::default(),
//             creation_timestamp: None,
//         })
//     }
//     Html {} => ProjectTaskConfig::ExportHtml(ExportHtmlTask { export }),
//     Markdown {} => ProjectTaskConfig::ExportMarkdown(ExportMarkdownTask {
// export }),     Text {} => ProjectTaskConfig::ExportText(ExportTextTask {
// export }),     Query { .. } => {
//         // todo: ignoring query task.
//         return None;
//     }
//     Svg { page } => {
//         // todo: ignoring page selection.
//         let _ = page;
//         return None;
//     }
//     Png { ppi, fill, page } => {
//         // todo: ignoring page fill.
//         let _ = fill;
//         // todo: ignoring page selection.
//         let _ = page;

//         let ppi = ppi.unwrap_or(144.) as f32;
//         let ppi = ppi.try_into().unwrap();
//         ProjectTaskConfig::ExportPng(ExportPngTask {
//             export,
//             ppi,
//             fill: None,
//         })
//     }
// };
/// User configuration for export.
#[derive(Debug, Clone)]
pub struct ExportUserConfig {
    /// The task configuration
    pub task: ProjectTask,
}

impl Default for ExportUserConfig {
    fn default() -> Self {
        Self {
            task: ProjectTask::ExportPdf(ExportPdfTask {
                export: ProjectExportTask {
                    when: TaskWhen::Never,
                    output: None,
                    transform: vec![],
                },
                pdf_standards: vec![],
                creation_timestamp: None,
            }),
        }
    }
}

#[derive(Clone)]
pub struct ExportTask {
    pub handle: tokio::runtime::Handle,
    pub group: String,
    pub editor_tx: Option<mpsc::UnboundedSender<EditorRequest>>,
    pub factory: SyncTaskFactory<ExportConfig>,
    export_folder: FutureFolder,
    count_word_folder: FutureFolder,
}

impl ExportTask {
    pub fn new(
        handle: tokio::runtime::Handle,
        group: String,
        editor_tx: Option<mpsc::UnboundedSender<EditorRequest>>,
        data: ExportConfig,
    ) -> Self {
        Self {
            handle,
            group,
            editor_tx,
            factory: SyncTaskFactory::new(data),
            export_folder: FutureFolder::default(),
            count_word_folder: FutureFolder::default(),
        }
    }

    pub fn change_config(&self, config: ExportUserConfig) {
        self.factory.mutate(|data| data.config = config);
    }

    pub fn signal(&self, snap: &LspCompiledArtifact) {
        //     self.factory.task().signal(snap, self);
        // }

        // fn signal(self: Arc<Self>, snap: &LspCompiledArtifact, t: &ExportTask) {
        let config = self.factory.task();

        self.signal_export(snap, &config);
        self.signal_count_word(snap, &config);
    }

    fn signal_export(
        &self,
        artifact: &LspCompiledArtifact,
        config: &Arc<ExportConfig>,
    ) -> Option<()> {
        let doc = artifact.doc.as_ref().ok()?;
        let s = artifact.signal;

        let when = config.config.task.when().unwrap_or_default();
        let need_export = (!matches!(when, TaskWhen::Never) && s.by_entry_update)
            || match when {
                TaskWhen::Never => false,
                TaskWhen::OnType => s.by_mem_events,
                TaskWhen::OnSave => s.by_fs_events,
                TaskWhen::OnDocumentHasTitle => s.by_fs_events && doc.info.title.is_some(),
            };

        if !need_export {
            return None;
        }

        let rev = artifact.world.revision().get();
        let fut = self.export_folder.spawn(rev, || {
            let task = config.config.task.clone();
            let artifact = artifact.clone();
            Box::pin(async move {
                log_err(
                    Self::do_export(ExportOnceTask {
                        task,
                        artifact,
                        lock_path: None,
                    })
                    .await,
                );
                Some(())
            })
        })?;

        self.handle.spawn(fut);

        Some(())
    }

    fn signal_count_word(
        &self,
        artifact: &LspCompiledArtifact,
        config: &Arc<ExportConfig>,
    ) -> Option<()> {
        if !config.count_words {
            return None;
        }

        let editor_tx = self.editor_tx.clone()?;
        let rev = artifact.world.revision().get();
        let fut = self.count_word_folder.spawn(rev, || {
            let artifact = artifact.clone();
            let group = self.group.clone();
            Box::pin(async move {
                let doc = artifact.doc.ok()?;
                let wc =
                    log_err(FutureFolder::compute(move |_| word_count::word_count(&doc)).await);
                log::debug!("WordCount({group}:{rev}): {wc:?}");

                if let Some(wc) = wc {
                    let _ = editor_tx.send(EditorRequest::WordCount(group, wc));
                }

                Some(())
            })
        })?;

        self.handle.spawn(fut);

        Some(())
    }

    async fn do_export(task: ExportOnceTask) -> anyhow::Result<Option<PathBuf>> {
        use reflexo_vec2svg::DefaultExportFeature;
        use ProjectTask::*;

        let ExportOnceTask {
            task,
            artifact: CompiledArtifact { snap, doc, .. },
            lock_path: lock_dir,
        } = task;

        // Prepare the output path.
        let entry = snap.world.entry_state();
        let config = task.as_export().unwrap();
        let output = config.output.clone().unwrap_or_default();
        let Some(to) = output.substitute(&entry) else {
            return Ok(None);
        };
        if to.is_relative() {
            bail!("RenderActor({task:?}): path is relative: {to:?}");
        }
        if to.is_dir() {
            bail!("RenderActor({task:?}): path is a directory: {to:?}");
        }
        let to = to.with_extension(task.extension());
        log::info!("RenderActor({task:?}): exporting {entry:?} to {to:?}");
        if let Some(e) = to.parent() {
            if !e.exists() {
                std::fs::create_dir_all(e).with_context(|| {
                    format!("RenderActor({task:?}): failed to create directory")
                })?;
            }
        }

        let _: Option<()> = lock_dir.and_then(|lock_dir| {
            let mut updater = crate::project::update_lock(lock_dir);

            let doc_id = updater.compiled(&snap.world)?;
            let task_id = doc_id.clone();

            // let when = self.config.when;

            // todo: page transforms
            // let transforms = vec![];

            // use tinymist_project::ExportTask as ProjectExportTask;

            // let export = ProjectExportTask {
            //     when,
            //     output: Some(self.config.output.clone()),
            //     transform: transforms,
            // };

            // let config = match kind {
            //     Pdf { creation_timestamp } => {
            //         let _ = creation_timestamp;
            //         ProjectTaskConfig::ExportPdf(ExportPdfTask {
            //             export,
            //             pdf_standards: Default::default(),
            //             creation_timestamp: None,
            //         })
            //     }
            //     Html {} => ProjectTaskConfig::ExportHtml(ExportHtmlTask { export }),
            //     Markdown {} => ProjectTaskConfig::ExportMarkdown(ExportMarkdownTask {
            // export }),     Text {} =>
            // ProjectTaskConfig::ExportText(ExportTextTask { export }),
            //     Query { .. } => {
            //         // todo: ignoring query task.
            //         return None;
            //     }
            //     Svg { page } => {
            //         // todo: ignoring page selection.
            //         let _ = page;
            //         return None;
            //     }
            //     Png { ppi, fill, page } => {
            //         // todo: ignoring page fill.
            //         let _ = fill;
            //         // todo: ignoring page selection.
            //         let _ = page;

            //         let ppi = ppi.unwrap_or(144.) as f32;
            //         let ppi = ppi.try_into().unwrap();
            //         ProjectTaskConfig::ExportPng(ExportPngTask {
            //             export,
            //             ppi,
            //             fill: None,
            //         })
            //     }
            // };

            let task = ApplyProjectTask {
                id: task_id,
                document: doc_id,
                task: task.clone(),
            };

            updater.task(task);
            updater.commit();

            Some(())
        });

        // Prepare the document.
        let doc = doc.map_err(|_| anyhow::anyhow!("no document"))?;

        // Prepare data.
        let kind2 = task.clone();
        let data = FutureFolder::compute(move |_| -> anyhow::Result<Vec<u8>> {
            let doc = &doc;

            // static BLANK: Lazy<Page> = Lazy::new(Page::default);
            let first_page = doc.pages.first().unwrap();
            Ok(match kind2 {
                Preview(..) => vec![],
                // todo: more pdf flags
                ExportPdf(ExportPdfTask {
                    creation_timestamp, ..
                }) => {
                    let creation_timestamp = creation_timestamp.map(|timestamp|   DateTime::from_timestamp(timestamp, 0).ok_or_else(|| anyhow::anyhow!("timestamp out of range")))
                    .transpose()?;
                 
                    let timestamp =
                        convert_datetime(creation_timestamp.unwrap_or_else(chrono::Utc::now));
                    // todo: Some(pdf_uri.as_str())
                    // todo: timestamp world.now()
                    typst_pdf::pdf(
                        doc,
                        &PdfOptions {
                            timestamp,
                            ..Default::default()
                        },
                    )
                    .map_err(|e| anyhow::anyhow!("failed to convert to pdf: {e:?}"))?
                }
                Query(QueryTask {
                    export: _,
                    format,
                    output_extension: _,
                    // strict,
                    selector,
                    field,
                    one,
                    // pretty,
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
                        serialize(&mapped, &format,  pretty).map(String::into_bytes)?
                    }
                }
                ExportHtml { .. } => {
                    reflexo_vec2svg::render_svg_html::<DefaultExportFeature>(doc).into_bytes()
                }
                ExportText { .. } => format!("{}", FullTextDigest(doc.clone())).into_bytes(),
                ExportMarkdown { .. } => {
                    let conv = Typlite::new(Arc::new(snap.world))
                        .convert()
                        .map_err(|e| anyhow::anyhow!("failed to convert to markdown: {e}"))?;

                    conv.as_bytes().to_owned()
                }
                ExportSvg {
                    // page: Merged { .. },
                    ..
                } => typst_svg::svg_merged(doc, Abs::zero()).into_bytes(),
                // page: First
                ExportSvg { .. } => typst_svg::svg(first_page).into_bytes(),
                // ExportPng {
                //     ppi,
                //     fill: _,
                //     page: First,
                // } => {
                //     let ppi = ppi.unwrap_or(144.) as f32;
                //     if ppi <= 1e-6 {
                //         bail!("invalid ppi: {ppi}");
                //     }

                //     typst_render::render(first_page, ppi / 72.)
                //         .encode_png()
                //         .map_err(|err| anyhow::anyhow!("failed to encode PNG ({err})"))?
                // }
                ExportPng(ExportPngTask {
                    export: _,
                    ppi,
                    fill,
                    // page: Merged { gap },
                }) => {
                    // .unwrap_or(144.) 
                    let ppi = ppi.to_f32();
                    if ppi <= 1e-6 {
                        bail!("invalid ppi: {ppi}");
                    }

                    let fill = if let Some(fill) = fill {
                        parse_color(fill).map_err(|err| anyhow::anyhow!("invalid fill ({err})"))?
                    } else {
                        Color::WHITE
                    };

                    // let gap = if let Some(gap) = gap {
                    //     parse_length(gap).map_err(|err| anyhow::anyhow!("invalid gap ({err})"))?
                    // } else {
                    //     Abs::zero()
                    // };

                    let gap = Abs::zero();

                    typst_render::render_merged(doc, ppi / 72., gap, Some(fill))
                        .encode_png()
                        .map_err(|err| anyhow::anyhow!("failed to encode PNG ({err})"))?
                }
            })
        });

        tokio::fs::write(&to, data.await??)
            .await
            .with_context(|| format!("RenderActor({task:?}): failed to export"))?;

        log::info!("RenderActor({task:?}): export complete");
        Ok(Some(to))
    }

    pub async fn oneshot(
        &self,
        snap: LspCompileSnapshot,
        task: ProjectTask,
        lock_path: Option<ImmutPath>,
    ) -> anyhow::Result<Option<PathBuf>> {
        let artifact = snap.compile();
        Self::do_export(ExportOnceTask {
            task,
            artifact,
            lock_path,
        })
        .await
    }
}

pub struct ExportOnceTask {
    pub task: ProjectTask,
    pub artifact: LspCompiledArtifact,
    pub lock_path: Option<ImmutPath>,
}

#[derive(Clone, Default)]
pub struct ExportConfig {
    pub config: ExportUserConfig,
    // pub kind: ExportKind,
    // pub config: ProjectTaskConfig,
    pub count_words: bool,
}

impl ExportConfig {}

fn parse_color(fill: String) -> anyhow::Result<Color> {
    match fill.as_str() {
        "black" => Ok(Color::BLACK),
        "white" => Ok(Color::WHITE),
        "red" => Ok(Color::RED),
        "green" => Ok(Color::GREEN),
        "blue" => Ok(Color::BLUE),
        hex if hex.starts_with('#') => {
            Color::from_str(&hex[1..]).map_err(|e| anyhow::anyhow!("failed to parse color: {e}"))
        }
        _ => bail!("invalid color: {fill}"),
    }
}

fn parse_length(gap: String) -> anyhow::Result<Abs> {
    let length = typst::syntax::parse_code(&gap);
    if length.erroneous() {
        bail!("invalid length: {gap}, errors: {:?}", length.errors());
    }

    let length: Option<ast::Numeric> = descendants(&length).into_iter().find_map(SyntaxNode::cast);

    let Some(length) = length else {
        bail!("not a length: {gap}");
    };

    let (value, unit) = length.get();
    match unit {
        ast::Unit::Pt => Ok(Abs::pt(value)),
        ast::Unit::Mm => Ok(Abs::mm(value)),
        ast::Unit::Cm => Ok(Abs::cm(value)),
        ast::Unit::In => Ok(Abs::inches(value)),
        _ => bail!("invalid unit: {unit:?} in {gap}"),
    }
}

/// Low performance but simple recursive iterator.
fn descendants(node: &SyntaxNode) -> impl IntoIterator<Item = &SyntaxNode> + '_ {
    let mut res = vec![];
    for child in node.children() {
        res.push(child);
        res.extend(descendants(child));
    }

    res
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

/// Convert [`chrono::DateTime`] to [`TypstDatetime`]
fn convert_datetime(date_time: chrono::DateTime<chrono::Utc>) -> Option<TypstDatetime> {
    use chrono::{Datelike, Timelike};
    TypstDatetime::from_ymd_hms(
        date_time.year(),
        date_time.month().try_into().ok()?,
        date_time.day().try_into().ok()?,
        date_time.hour().try_into().ok()?,
        date_time.minute().try_into().ok()?,
        date_time.second().try_into().ok()?,
    )
}

/// Serialize data to the output format.
fn serialize(data: &impl serde::Serialize, format: &str, pretty: bool) -> anyhow::Result<String> {
    Ok(match format {
        "json" if pretty => serde_json::to_string_pretty(data)?,
        "json" => serde_json::to_string(data)?,
        "yaml" => serde_yaml::to_string(&data)?,
        "txt" => {
            use serde_json::Value::*;
            let value = serde_json::to_value(data)?;
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
    use super::*;

    #[test]
    fn test_default_never() {
        let conf = ExportConfig::default();
        assert!(!conf.count_words);
        assert_eq!(conf.config.task.when(), None);
        assert_eq!(conf.config.task.when().unwrap_or_default(), TaskWhen::Never);
    }

    #[test]
    fn test_parse_length() {
        assert_eq!(parse_length("1pt".to_owned()).unwrap(), Abs::pt(1.));
        assert_eq!(parse_length("1mm".to_owned()).unwrap(), Abs::mm(1.));
        assert_eq!(parse_length("1cm".to_owned()).unwrap(), Abs::cm(1.));
        assert_eq!(parse_length("1in".to_owned()).unwrap(), Abs::inches(1.));
        assert!(parse_length("1".to_owned()).is_err());
        assert!(parse_length("1px".to_owned()).is_err());
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
}
