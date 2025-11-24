//! The computation for pdf export.

use tinymist_std::time::ToUtcDateTime;
use tinymist_world::args::PdfStandard;
pub use typst_pdf::PdfStandard as TypstPdfStandard;
pub use typst_pdf::pdf;

use typst_pdf::{PdfOptions, PdfStandards, Timestamp};

use super::*;
use crate::model::ExportPdfTask;

/// The computation for pdf export.
pub struct PdfExport;

impl<F: CompilerFeat> ExportComputation<F, TypstPagedDocument> for PdfExport {
    type Output = Bytes;
    type Config = ExportPdfTask;

    fn run(
        _graph: &Arc<WorldComputeGraph<F>>,
        doc: &Arc<TypstPagedDocument>,
        config: &ExportPdfTask,
    ) -> Result<Bytes> {
        let creation_timestamp = config
            .creation_timestamp
            .map(|ts| ts.to_utc_datetime().context("timestamp is out of range"))
            .transpose()?
            .unwrap_or_else(tinymist_std::time::utc_now);
        // todo: this seems different from `Timestamp::new_local` which also embeds the
        // timezone information.
        let timestamp = Timestamp::new_utc(tinymist_std::time::to_typst_time(creation_timestamp));

        let standards = PdfStandards::new(
            &config
                .pdf_standards
                .iter()
                .map(|standard| match standard {
                    tinymist_world::args::PdfStandard::V_1_4 => typst_pdf::PdfStandard::V_1_4,
                    tinymist_world::args::PdfStandard::V_1_5 => typst_pdf::PdfStandard::V_1_5,
                    tinymist_world::args::PdfStandard::V_1_6 => typst_pdf::PdfStandard::V_1_6,
                    tinymist_world::args::PdfStandard::V_1_7 => typst_pdf::PdfStandard::V_1_7,
                    tinymist_world::args::PdfStandard::V_2_0 => typst_pdf::PdfStandard::V_2_0,
                    tinymist_world::args::PdfStandard::A_1b => typst_pdf::PdfStandard::A_1b,
                    tinymist_world::args::PdfStandard::A_1a => typst_pdf::PdfStandard::A_1a,
                    tinymist_world::args::PdfStandard::A_2b => typst_pdf::PdfStandard::A_2b,
                    tinymist_world::args::PdfStandard::A_2u => typst_pdf::PdfStandard::A_2u,
                    tinymist_world::args::PdfStandard::A_2a => typst_pdf::PdfStandard::A_2a,
                    tinymist_world::args::PdfStandard::A_3b => typst_pdf::PdfStandard::A_3b,
                    tinymist_world::args::PdfStandard::A_3u => typst_pdf::PdfStandard::A_3u,
                    tinymist_world::args::PdfStandard::A_3a => typst_pdf::PdfStandard::A_3a,
                    tinymist_world::args::PdfStandard::A_4 => typst_pdf::PdfStandard::A_4,
                    tinymist_world::args::PdfStandard::A_4f => typst_pdf::PdfStandard::A_4f,
                    tinymist_world::args::PdfStandard::A_4e => typst_pdf::PdfStandard::A_4e,
                    tinymist_world::args::PdfStandard::Ua_1 => typst_pdf::PdfStandard::Ua_1,
                })
                .collect::<Vec<_>>(),
        )
        .context_ut("prepare pdf standards")?;

        let tagged = !config.no_pdf_tags && config.pages.is_none();
        // todo: emit warning diag
        if config.pages.is_some() && !config.no_pdf_tags {
            log::warn!(
                "the resulting PDF will be inaccessible because using --pages implies --no-pdf-tags"
            );
        }
        if !tagged {
            const ACCESSIBLE: &[(PdfStandard, &str)] = &[
                (PdfStandard::A_1a, "PDF/A-1a"),
                (PdfStandard::A_2a, "PDF/A-2a"),
                (PdfStandard::A_3a, "PDF/A-3a"),
                (PdfStandard::Ua_1, "PDF/UA-1"),
            ];

            for (standard, name) in ACCESSIBLE {
                if config.pdf_standards.contains(standard) {
                    if config.no_pdf_tags {
                        log::warn!("cannot disable PDF tags when exporting a {name} document");
                    } else {
                        log::warn!(
                            "cannot disable PDF tags when exporting a {name} document. Hint: using --pages implies --no-pdf-tags"
                        );
                    }
                }
            }
        }

        let options = PdfOptions {
            page_ranges: config
                .pages
                .as_ref()
                .map(|pages| exported_page_ranges(pages)),
            timestamp: Some(timestamp),
            standards,
            tagged,
            ..Default::default()
        };
        // log::info!("used options for pdf export: {options:?}");

        // todo: Some(pdf_uri.as_str())
        // todo: ident option
        Ok(Bytes::new(typst_pdf::pdf(doc, &options)?))
    }
}
