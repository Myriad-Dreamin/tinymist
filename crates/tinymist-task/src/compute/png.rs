//! The computation for png export.

use std::sync::Arc;

use tinymist_std::error::prelude::*;
use tinymist_std::typst::TypstPagedDocument;
use tinymist_world::{CompilerFeat, ExportComputation, WorldComputeGraph};
use typst::foundations::Bytes;

use crate::compute::{parse_color, parse_length, select_pages};
use crate::model::ExportPngTask;
use crate::{ImageOutput, PageMerge, PagedOutput};

/// The computation for png export.
pub struct PngExport;

impl<F: CompilerFeat> ExportComputation<F, TypstPagedDocument> for PngExport {
    type Output = ImageOutput<Bytes>;
    type Config = ExportPngTask;

    fn run(
        _graph: &Arc<WorldComputeGraph<F>>,
        doc: &Arc<TypstPagedDocument>,
        config: &ExportPngTask,
    ) -> Result<Self::Output> {
        let ppi = config.ppi.to_f32();
        if ppi <= 1e-6 {
            bail!("invalid ppi: {ppi}");
        }

        let fill = if let Some(fill) = &config.fill {
            Some(parse_color(fill).map_err(|err| anyhow::anyhow!("invalid fill ({err})"))?)
        } else {
            None
        };

        let ppp = ppi / 72.;

        let exported_pages = select_pages(doc, &config.pages);
        if let Some(PageMerge { ref gap }) = config.merge {
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
            let pixmap = typst_render::render_merged(&dummy_doc, ppp, gap, fill);
            let png = pixmap
                .encode_png()
                .map(Bytes::new)
                .context_ut("failed to encode PNG")?;
            Ok(ImageOutput::Merged(png))
        } else {
            let exported = exported_pages
                .into_iter()
                .map(|(i, page)| {
                    let pixmap = typst_render::render(page, ppp);
                    let png = pixmap
                        .encode_png()
                        .map(Bytes::new)
                        .context_ut("failed to encode PNG")?;
                    Ok(PagedOutput {
                        page: i,
                        value: png,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(ImageOutput::Paged(exported))
        }
    }
}

// impl<F: CompilerFeat> WorldComputable<F> for PngExport {
//     type Output = Option<Bytes>;

//     fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self::Output> {
//         OptionDocumentTask::run_export::<F, Self>(graph)
//     }
// }
