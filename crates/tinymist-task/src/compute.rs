//! The computations for the tasks.

use std::str::FromStr;
use std::sync::Arc;

use tinymist_std::error::prelude::*;
use tinymist_std::typst::TypstPagedDocument;
use tinymist_world::{CompileSnapshot, CompilerFeat, ExportComputation, WorldComputeGraph};
use typst::foundations::Bytes;
use typst::layout::{Abs, Page};
use typst::syntax::{SyntaxNode, ast};
use typst::visualize::Color;

use crate::{Pages, TaskWhen, exported_page_ranges};

mod html;
pub use html::*;
mod png;
pub use png::*;
mod query;
pub use query::*;
mod svg;
pub use svg::*;
#[cfg(feature = "pdf")]
pub mod pdf;
#[cfg(feature = "pdf")]
pub use pdf::*;
#[cfg(feature = "text")]
pub mod text;
#[cfg(feature = "text")]
pub use text::*;

/// The flag indicating that the svg export is needed.
pub struct SvgFlag;
/// The flag indicating that the png export is needed.
pub struct PngFlag;
/// The flag indicating that the html export is needed.
pub struct HtmlFlag;

/// The computation to check if the export is needed.
pub struct ExportTimings;

impl ExportTimings {
    /// Checks if the export is needed.
    pub fn needs_run<F: CompilerFeat, D: typst::Document>(
        snap: &CompileSnapshot<F>,
        timing: Option<&TaskWhen>,
        docs: Option<&D>,
    ) -> Option<bool> {
        snap.signal
            .should_run_task(timing.unwrap_or(&TaskWhen::Never), docs)
    }
}

pub enum ImageOutput<T> {
    Paged(Vec<PagedOutput<T>>),
    Merged(T),
}

pub struct PagedOutput<T> {
    pub page: usize,
    pub value: T,
}

fn select_pages<'a>(
    document: &'a TypstPagedDocument,
    pages: &Option<Vec<Pages>>,
) -> Vec<(usize, &'a Page)> {
    let pages = pages.as_ref().map(|pages| exported_page_ranges(pages));
    document
        .pages
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            pages
                .as_ref()
                .is_none_or(|exported_page_ranges| exported_page_ranges.includes_page_index(*i))
        })
        .collect::<Vec<_>>()
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

fn parse_color(fill: &str) -> anyhow::Result<Color> {
    match fill {
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

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_parse_color() {
        assert_eq!(parse_color("black").unwrap(), Color::BLACK);
        assert_eq!(parse_color("white").unwrap(), Color::WHITE);
        assert_eq!(parse_color("red").unwrap(), Color::RED);
        assert_eq!(parse_color("green").unwrap(), Color::GREEN);
        assert_eq!(parse_color("blue").unwrap(), Color::BLUE);
        assert_eq!(parse_color("#000000").unwrap().to_hex(), "#000000");
        assert_eq!(parse_color("#ffffff").unwrap().to_hex(), "#ffffff");
        assert_eq!(parse_color("#000000cc").unwrap().to_hex(), "#000000cc");
        assert!(parse_color("invalid").is_err());
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
