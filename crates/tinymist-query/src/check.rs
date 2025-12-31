use tinymist_lint::KnownIssues;
use tinymist_project::LspCompiledArtifact;

use crate::{DiagWorker, DiagnosticsMap, SemanticRequest, prelude::*};

/// A request to check the document for errors and lints.
#[derive(Clone)]
pub struct CheckRequest {
    /// The compilation result of the document.
    pub snap: LspCompiledArtifact,
}

/// A request to compute only lint diagnostics for the document.
#[cfg(feature = "lint-v2")]
#[derive(Clone)]
pub struct LintRequest {
    /// The compilation result of the document.
    pub snap: LspCompiledArtifact,
}

/// The diagnostics emitted by a full check run.
#[derive(Debug, Clone, Default)]
pub struct DiagnosticsResult {
    /// Diagnostics reported by the compiler.
    pub compiler: DiagnosticsMap,
    /// Diagnostics reported by lint passes.
    pub lint: DiagnosticsMap,
}

impl SemanticRequest for CheckRequest {
    type Response = DiagnosticsResult;

    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response> {
        let compiler_diags: Vec<_> = self.snap.diagnostics().cloned().collect();
        let known_issues = KnownIssues::from_compiler_diagnostics(compiler_diags.iter());
        let lint = DiagWorker::new(ctx).check(&known_issues).results;
        let compiler = DiagWorker::new(ctx).convert_all(compiler_diags.iter());

        Some(DiagnosticsResult { compiler, lint })
    }
}

#[cfg(feature = "lint-v2")]
impl SemanticRequest for LintRequest {
    type Response = DiagnosticsMap;

    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response> {
        let compiler_diags: Vec<_> = self.snap.diagnostics().cloned().collect();
        let known_issues = KnownIssues::from_compiler_diagnostics(compiler_diags.iter());
        Some(DiagWorker::new(ctx).check(&known_issues).results)
    }
}
