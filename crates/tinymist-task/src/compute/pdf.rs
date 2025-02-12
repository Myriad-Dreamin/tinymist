use super::*;

use crate::model::ExportPdfTask;
use tinymist_world::args::convert_source_date_epoch;
use typst::foundations::Datetime;
pub use typst_pdf::pdf;
use typst_pdf::PdfOptions;
pub use typst_pdf::PdfStandard as TypstPdfStandard;
use typst_pdf::Timestamp;
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
            .context_ut("parse pdf creation timestamp")?
            .unwrap_or_else(chrono::Utc::now);

        // todo: Some(pdf_uri.as_str())

        Ok(Bytes::new(typst_pdf::pdf(
            doc,
            &PdfOptions {
                timestamp: convert_datetime(creation_timestamp),
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

// impl<F: CompilerFeat> WorldComputable<F> for PdfExport {
//     type Output = Option<Bytes>;

//     fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self::Output> {
//         OptionDocumentTask::run_export::<F, Self>(graph)
//     }
// }

// use std::sync::Arc;

// use reflexo::typst::TypstPagedDocument;
// use typst::{diag:: World;
// use typst_pdf::{PdfOptions, PdfStandard, PdfStandards, Timestamp};

// #[derive(Debug, Clone, Default)]
// pub struct PdfDocExporter {
//     ctime: Option<Timestamp>,
//     standards: Option<PdfStandards>,
// }

// impl PdfDocExporter {
//     pub fn with_ctime(mut self, v: Option<Timestamp>) -> Self {
//         self.ctime = v;
//         self
//     }

//     pub fn with_standard(mut self, v: Option<PdfStandard>) -> Self {
//         self.standards = v.map(|v| PdfStandards::new(&[v]).unwrap());
//         self
//     }
// }

// impl Exporter<TypstPagedDocument, Vec<u8>> for PdfDocExporter {
//     fn export(&self, _world: &dyn World, output: Arc<TypstPagedDocument>) ->
// Vecu8>> {         // todo: ident option

//         typst_pdf::pdf(
//             output.as_ref(),
//             &PdfOptions {
//                 timestamp: self.ctime,
//                 standards: self.standards.clone().unwrap_or_default(),
//                 ..Default::default()
//             },
//         )
//     }
// }
