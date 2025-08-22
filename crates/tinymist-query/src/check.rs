use tinymist_lint::KnownLintIssues;
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
        let compiler_diags = self.snap.diagnostics();

        let known_issues = KnownLintIssues::from_compiler_diagnostics(compiler_diags.clone());
        Some(worker.full_check(&known_issues).convert_all(compiler_diags))
    }
}
