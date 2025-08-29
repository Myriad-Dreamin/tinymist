use tinymist_project::LspCompiledArtifact;

use crate::{DiagWorker, DiagnosticsMap, SemanticRequest, prelude::*};

/// A request to check the document for errors and lints.
#[derive(Clone)]
pub struct CheckRequest {
    /// The compilation result of the document.
    pub snap: LspCompiledArtifact,
}

impl SemanticRequest for CheckRequest {
    type Response = DiagnosticsMap;

    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response> {
        let worker = DiagWorker::new(ctx);
        Some(worker.full_check().convert_all(self.snap.diagnostics()))
    }
}
