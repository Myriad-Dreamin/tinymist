//! The computation for svg export.

use std::sync::Arc;

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
        let exported_pages = select_pages(doc, &config.pages);
        if let Some(PageMerge { ref gap }) = config.merge {
            // Typst does not expose svg-merging API.
            // Therefore, we have to create a dummy document here.
            let dummy_doc = TypstPagedDocument {
                pages: exported_pages
                    .into_iter()
                    .map(|(_, page)| page.clone())
                    .collect(),
                ..Default::default()
            };
            let gap = gap
                .as_ref()
                .and_then(|gap| parse_length(gap).ok())
                .unwrap_or_default();
            let svg = typst_svg::svg_merged(&dummy_doc, gap);
            Ok(ImageOutput::Merged(svg))
        } else {
            let exported = exported_pages
                .into_iter()
                .map(|(i, page)| {
                    let svg = typst_svg::svg(page);
                    Ok(PagedOutput {
                        page: i,
                        value: svg,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(ImageOutput::Paged(exported))
        }
    }
}

// impl<F: CompilerFeat> WorldComputable<F> for SvgExport {
//     type Output = Option<String>;

//     fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self::Output> {
//         OptionDocumentTask::run_export::<F, Self>(graph)
//     }
// }
