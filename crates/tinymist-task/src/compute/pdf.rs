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
        let options = pdf_options(
            config.pages.as_deref(),
            &config.pdf_standards,
            config.no_pdf_tags,
            config.creation_timestamp,
        )?;

        // log::info!("used options for pdf export: {options:?}");

        // todo: Some(pdf_uri.as_str())
        // todo: ident option
        Ok(Bytes::new(typst_pdf::pdf(doc, &options)?))
    }
}

/// Creates PDF options from shared project export arguments.
pub fn pdf_options(
    pages: Option<&[Pages]>,
    pdf_standards: &[PdfStandard],
    no_pdf_tags: bool,
    creation_timestamp: Option<i64>,
) -> Result<PdfOptions> {
    let creation_timestamp = creation_timestamp
        .map(|ts| ts.to_utc_datetime().context("timestamp is out of range"))
        .transpose()?
        .unwrap_or_else(tinymist_std::time::utc_now);
    // todo: this seems different from `Timestamp::new_local` which also embeds the
    // timezone information.
    let timestamp = Timestamp::new_utc(tinymist_std::time::to_typst_time(creation_timestamp));

    let standards = PdfStandards::new(
        &pdf_standards
            .iter()
            .map(|standard| match standard {
                PdfStandard::V_1_4 => typst_pdf::PdfStandard::V_1_4,
                PdfStandard::V_1_5 => typst_pdf::PdfStandard::V_1_5,
                PdfStandard::V_1_6 => typst_pdf::PdfStandard::V_1_6,
                PdfStandard::V_1_7 => typst_pdf::PdfStandard::V_1_7,
                PdfStandard::V_2_0 => typst_pdf::PdfStandard::V_2_0,
                PdfStandard::A_1b => typst_pdf::PdfStandard::A_1b,
                PdfStandard::A_1a => typst_pdf::PdfStandard::A_1a,
                PdfStandard::A_2b => typst_pdf::PdfStandard::A_2b,
                PdfStandard::A_2u => typst_pdf::PdfStandard::A_2u,
                PdfStandard::A_2a => typst_pdf::PdfStandard::A_2a,
                PdfStandard::A_3b => typst_pdf::PdfStandard::A_3b,
                PdfStandard::A_3u => typst_pdf::PdfStandard::A_3u,
                PdfStandard::A_3a => typst_pdf::PdfStandard::A_3a,
                PdfStandard::A_4 => typst_pdf::PdfStandard::A_4,
                PdfStandard::A_4f => typst_pdf::PdfStandard::A_4f,
                PdfStandard::A_4e => typst_pdf::PdfStandard::A_4e,
                PdfStandard::Ua_1 => typst_pdf::PdfStandard::Ua_1,
            })
            .collect::<Vec<_>>(),
    )
    .map_err(|err| err.message().clone())
    .context("prepare pdf standards")?;

    let tagged = !no_pdf_tags && pages.is_none();
    // todo: emit warning diag
    if pages.is_some() && !no_pdf_tags {
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
            if pdf_standards.contains(standard) {
                if no_pdf_tags {
                    log::warn!("cannot disable PDF tags when exporting a {name} document");
                } else {
                    log::warn!(
                        "cannot disable PDF tags when exporting a {name} document. Hint: using --pages implies --no-pdf-tags"
                    );
                }
            }
        }
    }

    Ok(PdfOptions {
        page_ranges: pages.map(exported_page_ranges),
        timestamp: Some(timestamp),
        standards,
        tagged,
        ..Default::default()
    })
}
