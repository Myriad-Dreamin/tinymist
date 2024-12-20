//! The actor that handles various document export, like PDF and SVG export.

use std::str::FromStr;
use std::{path::PathBuf, sync::Arc};

use anyhow::{bail, Context};
use reflexo_typst::{EntryReader, EntryState, TaskInputs, TypstDatetime};
use tinymist_query::{ExportKind, PageSelection};
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
use crate::{
    actor::{
        editor::EditorRequest,
        typ_client::WorldSnapFut,
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
        snap: WorldSnapFut,
        entry: Option<EntryState>,
        kind: ExportKind,
    ) -> impl Future<Output = anyhow::Result<Option<PathBuf>>> {
        let export = self.factory.task();
        async move {
            let snap = snap.receive().await?;
            let snap = snap.task(TaskInputs {
                entry,
                ..Default::default()
            });

            let artifact = snap.compile().await;
            export.do_export(&kind, artifact).await
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
                ExportMode::OnDocumentHasTitle => s.by_fs_events && doc.info.title.is_some(),
            };

        if !need_export {
            return None;
        }

        t.export_folder.spawn(artifact.world.revision().get(), || {
            let this = self.clone();
            let artifact = artifact.clone();
            Box::pin(async move {
                log_err(this.do_export(&this.kind, artifact).await);
                Some(())
            })
        });

        Some(())
    }

    fn signal_count_word(&self, artifact: &CompiledArtifact<LspCompilerFeat>, t: &ExportTask) {
        if !self.count_words {
            return;
        }

        let Some(editor_tx) = self.editor_tx.clone() else {
            return;
        };
        let revision = artifact.world.revision().get();

        t.count_word_folder.spawn(revision, || {
            let artifact = artifact.clone();
            let group = self.group.clone();
            Box::pin(async move {
                let doc = artifact.doc.ok()?;
                let wc =
                    log_err(FutureFolder::compute(move |_| word_count::word_count(&doc)).await);
                log::debug!("WordCount({group}:{revision}): {wc:?}");

                if let Some(wc) = wc {
                    let _ = editor_tx.send(EditorRequest::WordCount(group, wc));
                }

                Some(())
            })
        });
    }

    async fn do_export(
        &self,
        kind: &ExportKind,
        artifact: CompiledArtifact<LspCompilerFeat>,
    ) -> anyhow::Result<Option<PathBuf>> {
        use reflexo_vec2svg::DefaultExportFeature;
        use ExportKind::*;
        use PageSelection::*;

        let CompiledArtifact { snap, doc, .. } = artifact;

        // Prepare the output path.
        let entry = snap.world.entry_state();
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

        // Prepare the document.
        let doc = doc.map_err(|_| anyhow::anyhow!("no document"))?;

        // Prepare data.
        let kind2 = kind.clone();
        let data = FutureFolder::compute(move |_| -> anyhow::Result<Vec<u8>> {
            let doc = &doc;

            // static BLANK: Lazy<Page> = Lazy::new(Page::default);
            let first_page = doc.pages.first().unwrap();
            Ok(match kind2 {
                Pdf { creation_timestamp } => {
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
                Query {
                    format,
                    output_extension: _,
                    strict,
                    selector,
                    field,
                    one,
                    pretty,
                } => {
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
                        serialize(value, &format, strict, pretty).map(String::into_bytes)?
                    } else {
                        serialize(&mapped, &format, strict, pretty).map(String::into_bytes)?
                    }
                }
                Html {} => {
                    reflexo_vec2svg::render_svg_html::<DefaultExportFeature>(doc).into_bytes()
                }
                Text {} => format!("{}", FullTextDigest(doc.clone())).into_bytes(),
                Markdown {} => {
                    let conv = Typlite::new(Arc::new(snap.world))
                        .convert()
                        .map_err(|e| anyhow::anyhow!("failed to convert to markdown: {e}"))?;

                    conv.as_bytes().to_owned()
                }
                Svg { page: First } => typst_svg::svg(first_page).into_bytes(),
                Svg {
                    page: Merged { .. },
                } => typst_svg::svg_merged(doc, Abs::zero()).into_bytes(),
                Png {
                    ppi,
                    fill: _,
                    page: First,
                } => {
                    let ppi = ppi.unwrap_or(144.) as f32;
                    if ppi <= 1e-6 {
                        bail!("invalid ppi: {ppi}");
                    }

                    typst_render::render(first_page, ppi / 72.)
                        .encode_png()
                        .map_err(|err| anyhow::anyhow!("failed to encode PNG ({err})"))?
                }
                Png {
                    ppi,
                    fill,
                    page: Merged { gap },
                } => {
                    let ppi = ppi.unwrap_or(144.) as f32;
                    if ppi <= 1e-6 {
                        bail!("invalid ppi: {ppi}");
                    }

                    let fill = if let Some(fill) = fill {
                        parse_color(fill).map_err(|err| anyhow::anyhow!("invalid fill ({err})"))?
                    } else {
                        Color::WHITE
                    };

                    let gap = if let Some(gap) = gap {
                        parse_length(gap).map_err(|err| anyhow::anyhow!("invalid gap ({err})"))?
                    } else {
                        Abs::zero()
                    };

                    typst_render::render_merged(doc, ppi / 72., gap, Some(fill))
                        .encode_png()
                        .map_err(|err| anyhow::anyhow!("failed to encode PNG ({err})"))?
                }
            })
        });

        tokio::fs::write(&to, data.await??)
            .await
            .with_context(|| format!("RenderActor({kind:?}): failed to export"))?;

        log::info!("RenderActor({kind:?}): export complete");
        Ok(Some(to))
    }
}

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
fn serialize(
    data: &impl serde::Serialize,
    format: &str,
    strict: bool,
    pretty: bool,
) -> anyhow::Result<String> {
    Ok(match format {
        "json" if pretty => serde_json::to_string_pretty(data)?,
        "json" => serde_json::to_string(data)?,
        "yaml" => serde_yaml::to_string(&data)?,
        format if format == "txt" || !strict => {
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
        assert_eq!(conf.config.mode, ExportMode::Never);
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
