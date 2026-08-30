//! The computation for svg export.

use std::sync::Arc;

use reflexo_vec2svg::{SvgExportFeature, SvgExporter};
use tinymist_std::error::prelude::*;
use tinymist_std::typst::TypstPagedDocument;
use tinymist_world::{CompilerFeat, ExportComputation, WorldComputeGraph};

use crate::compute::{parse_length, select_pages};
use crate::model::ExportSvgTask;
use crate::{ImageOutput, PageMerge, PagedOutput};

/// The computation for svg export.
pub struct SvgExport;

impl<F: CompilerFeat> ExportComputation<F, TypstPagedDocument> for SvgExport {
    type Output = ImageOutput<String>;
    type Config = ExportSvgTask;

    fn run(
        _graph: &Arc<WorldComputeGraph<F>>,
        doc: &Arc<TypstPagedDocument>,
        config: &ExportSvgTask,
    ) -> Result<Self::Output> {
        render(doc, config)
    }
}

fn render(doc: &TypstPagedDocument, config: &ExportSvgTask) -> Result<ImageOutput<String>> {
    let exported_pages = select_pages(doc, &config.pages);
    let mut vector_doc = SvgExporter::<SvgExportFeature>::svg_doc(doc);
    vector_doc.module.prepare_glyphs();
    if let Some(PageMerge { ref gap }) = config.merge {
        let gap = gap
            .as_ref()
            .and_then(|gap| parse_length(gap).ok())
            .unwrap_or_default()
            .to_pt() as f32;
        let mut pages = exported_pages
            .into_iter()
            .map(|(i, _)| vector_doc.pages[i].clone())
            .collect::<Vec<_>>();
        let last = pages.len().saturating_sub(1);
        for page in pages.iter_mut().take(last) {
            page.size.y.0 += gap;
        }
        Ok(ImageOutput::Merged(
            SvgExporter::<SvgExportFeature>::render_flat_svg(&vector_doc.module, &pages, None),
        ))
    } else {
        let exported = exported_pages
            .into_iter()
            .map(|(i, _)| {
                let svg = SvgExporter::<SvgExportFeature>::render_flat_svg(
                    &vector_doc.module,
                    std::slice::from_ref(&vector_doc.pages[i]),
                    None,
                );
                Ok(PagedOutput {
                    page: i,
                    value: svg,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(ImageOutput::Paged(exported))
    }
}

// impl<F: CompilerFeat> WorldComputable<F> for SvgExport {
//     type Output = Option<String>;

//     fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self::Output> {
//         OptionDocumentTask::run_export::<F, Self>(graph)
//     }
// }

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use ecow::eco_vec;
    use typst::foundations::{Bytes, Content, Smart};
    use typst::layout::{Abs, Frame, FrameItem, Point, Sides, Size};
    use typst::syntax::Span;
    use typst::visualize::{ExchangeFormat, Image, RasterImage};
    use typst_layout::Page;

    use super::*;
    use crate::Pages;

    fn page(width: f64, height: f64, number: u64) -> Page {
        Page {
            frame: Frame::soft(Size::new(Abs::pt(width), Abs::pt(height))),
            bleed: Sides::default(),
            fill: Smart::Auto,
            numbering: None,
            supplement: Content::default(),
            number,
        }
    }

    fn document() -> TypstPagedDocument {
        TypstPagedDocument::new(
            eco_vec![page(10.0, 30.0, 1), page(20.0, 40.0, 2)],
            Default::default(),
        )
    }

    #[test]
    fn renders_pages_separately() {
        let ImageOutput::Paged(pages) = render(&document(), &ExportSvgTask::default()).unwrap()
        else {
            panic!("expected paged SVG output");
        };

        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].page, 0);
        assert!(pages[0].value.contains(r#"viewBox="0 0 10.000 30.000""#));
        assert_eq!(pages[1].page, 1);
        assert!(pages[1].value.contains(r#"viewBox="0 0 20.000 40.000""#));
    }

    #[test]
    fn renders_only_selected_pages() {
        let config = ExportSvgTask {
            pages: Some(vec![Pages::from_str("2").unwrap()]),
            ..Default::default()
        };
        let ImageOutput::Paged(pages) = render(&document(), &config).unwrap() else {
            panic!("expected paged SVG output");
        };

        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].page, 1);
        assert!(pages[0].value.contains(r#"viewBox="0 0 20.000 40.000""#));
    }

    #[test]
    fn merges_pages_with_configured_gap() {
        let config = ExportSvgTask {
            merge: Some(PageMerge {
                gap: Some("5pt".into()),
            }),
            ..Default::default()
        };
        let ImageOutput::Merged(svg) = render(&document(), &config).unwrap() else {
            panic!("expected merged SVG output");
        };

        assert!(svg.contains(r#"viewBox="0 0 20.000 75.000""#));
    }

    #[test]
    fn deduplicates_repeated_raster_images() {
        let data = Bytes::new(include_bytes!(
            "../../../../editors/vscode/icons/typst-small.png"
        ));
        let image = Image::plain(RasterImage::plain(data, ExchangeFormat::Png).unwrap());
        let image_size = Size::new(Abs::pt(10.0), Abs::pt(10.0));
        let mut page = page(40.0, 20.0, 1);
        page.frame.push(
            Point::zero(),
            FrameItem::Image(image.clone(), image_size, Span::detached()),
        );
        page.frame.push(
            Point::new(Abs::pt(20.0), Abs::zero()),
            FrameItem::Image(image, image_size, Span::detached()),
        );
        let document = TypstPagedDocument::new(eco_vec![page], Default::default());

        let ImageOutput::Paged(pages) = render(&document, &ExportSvgTask::default()).unwrap()
        else {
            panic!("expected paged SVG output");
        };
        let svg = &pages[0].value;

        assert_eq!(svg.matches("data:image/png;base64,").count(), 1);
        assert_eq!(svg.matches(r#"<image id="i"#).count(), 1);
        assert_eq!(svg.matches(r#"<use class="typst-image"#).count(), 2);
    }
}
