pub use typst_pdf::pdf;
pub use typst_pdf::PdfStandard as TypstPdfStandard;

use tinymist_world::args::convert_source_date_epoch;
use typst::foundations::Datetime;
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
        // todo: timestamp world.now()
        let creation_timestamp = config
            .creation_timestamp
            .map(convert_source_date_epoch)
            .transpose()
            .context_ut("prepare pdf creation timestamp")?
            .unwrap_or_else(chrono::Utc::now);

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
                timestamp: convert_datetime(creation_timestamp),
                standards,
                ..Default::default()
            },
        )?))
    }
}

/// Convert [`chrono::DateTime`] to [`Timestamp`]
pub fn convert_datetime(date_time: chrono::DateTime<chrono::Utc>) -> Option<Timestamp> {
    use chrono::{Datelike, Timelike};
    Some(Timestamp::new_utc(Datetime::from_ymd_hms(
        date_time.year(),
        date_time.month().try_into().ok()?,
        date_time.day().try_into().ok()?,
        date_time.hour().try_into().ok()?,
        date_time.minute().try_into().ok()?,
        date_time.second().try_into().ok()?,
    )?))
}
