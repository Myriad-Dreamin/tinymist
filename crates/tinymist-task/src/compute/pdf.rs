use tinymist_std::time::ToUtcDateTime;
pub use typst_pdf::pdf;
pub use typst_pdf::PdfStandard as TypstPdfStandard;

use typst_pdf::{PdfOptions, PdfStandards, Timestamp};

use super::*;
use crate::model::ExportPdfTask;

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
        let timestamp = Timestamp::new_utc(tinymist_std::time::to_typst_time(creation_timestamp));

        let standards = PdfStandards::new(
            &config
                .pdf_standards
                .iter()
                .map(|standard| match standard {
                    tinymist_world::args::PdfStandard::V_1_7 => typst_pdf::PdfStandard::V_1_7,
                    tinymist_world::args::PdfStandard::A_2b => typst_pdf::PdfStandard::A_2b,
                    tinymist_world::args::PdfStandard::A_3b => typst_pdf::PdfStandard::A_3b,
                })
                .collect::<Vec<_>>(),
        )
        .context_ut("prepare pdf standards")?;

        // todo: Some(pdf_uri.as_str())
        // todo: ident option
        Ok(Bytes::new(typst_pdf::pdf(
            doc,
            &PdfOptions {
                timestamp: Some(timestamp),
                standards,
                ..Default::default()
            },
        )?))
    }
}
