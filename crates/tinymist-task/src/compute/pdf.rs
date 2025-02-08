use super::*;

pub use typst_pdf::pdf;
pub use typst_pdf::PdfStandard as TypstPdfStandard;
pub use typst_pdf::Timestamp as TypstTimestamp;
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
