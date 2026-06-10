use tinymist_lint::KnownIssues;
use tinymist_project::LspCompiledArtifact;
use tinymist_world::vfs::WorkspaceResolver;
use typst::diag::SourceDiagnostic;

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

        let known_issues = KnownIssues::from_compiler_diagnostics(compiler_diags.clone());
        Some(worker.check(&known_issues).convert_all(compiler_diags))
    }
}

/// Collects Tinymist lint diagnostics for the main document and its local Typst
/// dependencies.
pub fn collect_lint_diagnostics<'a>(
    ctx: &mut LocalContext,
    compiler_diags: impl IntoIterator<Item = &'a SourceDiagnostic>,
) -> EcoVec<SourceDiagnostic> {
    let known_issues = KnownIssues::from_compiler_diagnostics(compiler_diags.into_iter());
    let mut diagnostics = EcoVec::new();

    for dep in ctx.world().depended_files() {
        if WorkspaceResolver::is_package_file(dep)
            || dep
                .vpath()
                .as_rooted_path_compat()
                .extension()
                .is_none_or(|e| e != "typ")
        {
            continue;
        }

        let Ok(source) = ctx.world().source(dep) else {
            continue;
        };

        diagnostics.extend(ctx.lint(&source, &known_issues));
    }

    diagnostics
}
