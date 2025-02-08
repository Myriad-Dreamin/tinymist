#![allow(missing_docs)]

use std::str::FromStr;
use std::sync::Arc;

use comemo::Track;
use ecow::EcoString;
use tinymist_std::error::prelude::*;
use tinymist_std::typst::{TypstHtmlDocument, TypstPagedDocument};
use tinymist_world::{
    args::convert_source_date_epoch, CompileSnapshot, CompilerFeat, ExportComputation,
    WorldComputeGraph,
};
use typst::diag::{SourceResult, StrResult};
use typst::foundations::{Bytes, Content, Datetime, IntoValue, LocatableSelector, Scope, Value};
use typst::layout::Abs;
use typst::routines::EvalMode;
use typst::syntax::{ast, Span, SyntaxNode};
use typst::visualize::Color;
use typst::World;
use typst_eval::eval_string;
use typst_pdf::{PdfOptions, Timestamp};

use crate::model::{ExportHtmlTask, ExportPdfTask, ExportPngTask, ExportSvgTask};
use crate::primitives::TaskWhen;
use crate::{ExportTransform, Pages, QueryTask};

#[cfg(feature = "pdf")]
pub mod pdf;
#[cfg(feature = "pdf")]
pub use pdf::*;
#[cfg(feature = "text")]
pub mod text;
#[cfg(feature = "text")]
pub use text::*;

pub struct SvgFlag;
pub struct PngFlag;
pub struct HtmlFlag;

pub struct ExportTimings;

impl ExportTimings {
    pub fn needs_run<F: CompilerFeat, D: typst::Document>(
        snap: &CompileSnapshot<F>,
        timing: Option<TaskWhen>,
        docs: Option<&D>,
    ) -> Option<bool> {
        let s = snap.signal;
        let when = timing.unwrap_or(TaskWhen::Never);
        if !matches!(when, TaskWhen::Never) && s.by_entry_update {
            return Some(true);
        }

        match when {
            TaskWhen::Never => Some(false),
            TaskWhen::OnType => Some(s.by_mem_events),
            TaskWhen::OnSave => Some(s.by_fs_events),
            TaskWhen::OnDocumentHasTitle if s.by_fs_events => {
                docs.map(|doc| doc.info().title.is_some())
            }
            TaskWhen::OnDocumentHasTitle => Some(false),
        }
    }
}

pub struct SvgExport;

impl<F: CompilerFeat> ExportComputation<F, TypstPagedDocument> for SvgExport {
    type Output = String;
    type Config = ExportSvgTask;

    fn run(
        _graph: &Arc<WorldComputeGraph<F>>,
        doc: &Arc<TypstPagedDocument>,
        config: &ExportSvgTask,
    ) -> Result<String> {
        let (is_first, merged_gap) = get_page_selection(&config.export)?;

        let first_page = doc.pages.first();

        Ok(if is_first {
            if let Some(first_page) = first_page {
                typst_svg::svg(first_page)
            } else {
                typst_svg::svg_merged(doc, merged_gap)
            }
        } else {
            typst_svg::svg_merged(doc, merged_gap)
        })
    }
}

// impl<F: CompilerFeat> WorldComputable<F> for SvgExport {
//     type Output = Option<String>;

//     fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self::Output> {
//         OptionDocumentTask::run_export::<F, Self>(graph)
//     }
// }

pub struct PngExport;

impl<F: CompilerFeat> ExportComputation<F, TypstPagedDocument> for PngExport {
    type Output = Bytes;
    type Config = ExportPngTask;

    fn run(
        _graph: &Arc<WorldComputeGraph<F>>,
        doc: &Arc<TypstPagedDocument>,
        config: &ExportPngTask,
    ) -> Result<Bytes> {
        let ppi = config.ppi.to_f32();
        if ppi <= 1e-6 {
            tinymist_std::bail!("invalid ppi: {ppi}");
        }

        let fill = if let Some(fill) = &config.fill {
            parse_color(fill.clone()).map_err(|err| anyhow::anyhow!("invalid fill ({err})"))?
        } else {
            Color::WHITE
        };

        let (is_first, merged_gap) = get_page_selection(&config.export)?;

        let ppp = ppi / 72.;
        let pixmap = if is_first {
            if let Some(first_page) = doc.pages.first() {
                typst_render::render(first_page, ppp)
            } else {
                typst_render::render_merged(doc, ppp, merged_gap, Some(fill))
            }
        } else {
            typst_render::render_merged(doc, ppp, merged_gap, Some(fill))
        };

        pixmap
            .encode_png()
            .map(Bytes::new)
            .context_ut("failed to encode PNG")
    }
}

// impl<F: CompilerFeat> WorldComputable<F> for PngExport {
//     type Output = Option<Bytes>;

//     fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self::Output> {
//         OptionDocumentTask::run_export::<F, Self>(graph)
//     }
// }

pub struct HtmlExport;

impl<F: CompilerFeat> ExportComputation<F, TypstHtmlDocument> for HtmlExport {
    type Output = String;
    type Config = ExportHtmlTask;

    fn run(
        _graph: &Arc<WorldComputeGraph<F>>,
        doc: &Arc<TypstHtmlDocument>,
        _config: &ExportHtmlTask,
    ) -> Result<String> {
        Ok(typst_html::html(doc)?)
    }
}

// impl<F: CompilerFeat> WorldComputable<F> for HtmlExport {
//     type Output = Option<String>;

//     fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self::Output> {
//         OptionDocumentTask::run_export::<F, Self>(graph)
//     }
// }

pub struct DocumentQuery;

impl DocumentQuery {
    // todo: query exporter
    /// Retrieve the matches for the selector.
    pub fn retrieve<D: typst::Document>(
        world: &dyn World,
        selector: &str,
        document: &D,
    ) -> StrResult<Vec<Content>> {
        let selector = eval_string(
            &typst::ROUTINES,
            world.track(),
            selector,
            Span::detached(),
            EvalMode::Code,
            Scope::default(),
        )
        .map_err(|errors| {
            let mut message = EcoString::from("failed to evaluate selector");
            for (i, error) in errors.into_iter().enumerate() {
                message.push_str(if i == 0 { ": " } else { ", " });
                message.push_str(&error.message);
            }
            message
        })?
        .cast::<LocatableSelector>()
        .map_err(|e| EcoString::from(format!("failed to cast: {}", e.message())))?;

        Ok(document
            .introspector()
            .query(&selector.0)
            .into_iter()
            .collect::<Vec<_>>())
    }

    fn run_inner<F: CompilerFeat, D: typst::Document>(
        g: &Arc<WorldComputeGraph<F>>,
        doc: &Arc<D>,
        config: &QueryTask,
    ) -> Result<Vec<Value>> {
        let selector = &config.selector;
        let elements = Self::retrieve(&g.snap.world, selector, doc.as_ref())
            .map_err(|e| anyhow::anyhow!("failed to retrieve: {e}"))?;
        if config.one && elements.len() != 1 {
            bail!("expected exactly one element, found {}", elements.len());
        }

        Ok(elements
            .into_iter()
            .filter_map(|c| match &config.field {
                Some(field) => c.get_by_name(field).ok(),
                _ => Some(c.into_value()),
            })
            .collect())
    }

    pub fn get_as_value<F: CompilerFeat, D: typst::Document>(
        g: &Arc<WorldComputeGraph<F>>,
        doc: &Arc<D>,
        config: &QueryTask,
    ) -> Result<serde_json::Value> {
        let mapped = Self::run_inner(g, doc, config)?;

        let res = if config.one {
            let Some(value) = mapped.first() else {
                bail!("no such field found for element");
            };
            serde_json::to_value(value)
        } else {
            serde_json::to_value(&mapped)
        };

        res.context("failed to serialize")
    }
}

impl<F: CompilerFeat, D: typst::Document> ExportComputation<F, D> for DocumentQuery {
    type Output = SourceResult<String>;
    type Config = QueryTask;

    fn run(
        g: &Arc<WorldComputeGraph<F>>,
        doc: &Arc<D>,
        config: &QueryTask,
    ) -> Result<SourceResult<String>> {
        let pretty = false;
        let mapped = Self::run_inner(g, doc, config)?;

        let res = if config.one {
            let Some(value) = mapped.first() else {
                bail!("no such field found for element");
            };
            serialize(value, &config.format, pretty)
        } else {
            serialize(&mapped, &config.format, pretty)
        };

        res.map(Ok)
    }
}

/// Serialize data to the output format.
fn serialize(data: &impl serde::Serialize, format: &str, pretty: bool) -> Result<String> {
    Ok(match format {
        "json" if pretty => serde_json::to_string_pretty(data).context("serialize query")?,
        "json" => serde_json::to_string(data).context("serialize query")?,
        "yaml" => serde_yaml::to_string(&data).context_ut("serialize query")?,
        "txt" => {
            use serde_json::Value::*;
            let value = serde_json::to_value(data).context("serialize query")?;
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

/// Gets legacy page selection
pub fn get_page_selection(task: &crate::ExportTask) -> Result<(bool, Abs)> {
    let is_first = task
        .transform
        .iter()
        .any(|t| matches!(t, ExportTransform::Pages { ranges, .. } if ranges == &[Pages::FIRST]));

    let mut gap_res = Abs::default();
    if !is_first {
        for trans in &task.transform {
            if let ExportTransform::Merge { gap } = trans {
                let gap = gap
                    .as_deref()
                    .map(parse_length)
                    .transpose()
                    .context_ut("failed to parse gap")?;
                gap_res = gap.unwrap_or_default();
            }
        }
    }

    Ok((is_first, gap_res))
}

fn parse_length(gap: &str) -> Result<Abs> {
    let length = typst::syntax::parse_code(gap);
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
        _ => anyhow::bail!("invalid color: {fill}"),
    }
}

/// Convert [`chrono::DateTime`] to [`Timestamp`]
fn convert_datetime(date_time: chrono::DateTime<chrono::Utc>) -> Option<Timestamp> {
    use chrono::{Datelike, Timelike};
    let datetime = Datetime::from_ymd_hms(
        date_time.year(),
        date_time.month().try_into().ok()?,
        date_time.day().try_into().ok()?,
        date_time.hour().try_into().ok()?,
        date_time.minute().try_into().ok()?,
        date_time.second().try_into().ok()?,
    );

    Some(Timestamp::new_utc(datetime.unwrap()))
}

#[cfg(test)]
mod tests {

    use super::*;

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
    fn test_parse_length() {
        assert_eq!(parse_length("1pt").unwrap(), Abs::pt(1.));
        assert_eq!(parse_length("1mm").unwrap(), Abs::mm(1.));
        assert_eq!(parse_length("1cm").unwrap(), Abs::cm(1.));
        assert_eq!(parse_length("1in").unwrap(), Abs::inches(1.));
        assert!(parse_length("1").is_err());
        assert!(parse_length("1px").is_err());
    }
}
