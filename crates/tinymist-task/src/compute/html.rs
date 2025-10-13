//! The computation for html export.

use std::sync::Arc;

use tinymist_std::error::prelude::*;
use tinymist_std::typst::TypstHtmlDocument;
use tinymist_world::{CompilerFeat, ExportComputation, WorldComputeGraph};

use crate::model::ExportHtmlTask;

/// The computation for html export.
pub struct HtmlExport;

impl<F: CompilerFeat> ExportComputation<F, TypstHtmlDocument> for HtmlExport {
    type Output = String;
    type Config = ExportHtmlTask;

    fn run(
        _graph: &Arc<WorldComputeGraph<F>>,
        doc: &Arc<TypstHtmlDocument>,
        _config: &ExportHtmlTask,
    ) -> Result<String> {
        Ok(typst_html::html(doc)?)
    }
}

// impl<F: CompilerFeat> WorldComputable<F> for HtmlExport {
//     type Output = Option<String>;

//     fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self::Output> {
//         OptionDocumentTask::run_export::<F, Self>(graph)
//     }
// }
