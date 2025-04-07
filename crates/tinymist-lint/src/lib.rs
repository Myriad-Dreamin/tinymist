//! A linter for Typst.

use typst::{
    diag::SourceDiagnostic,
    ecow::EcoVec,
    syntax::{Source, SyntaxNode},
};

/// A type alias for a vector of diagnostics.
type DiagnosticVec = EcoVec<SourceDiagnostic>;

/// Lints a Typst source and returns a vector of diagnostics.
pub fn lint_source(source: &Source) -> DiagnosticVec {
    SourceLinter::new().lint(source.root())
}

struct SourceLinter {
    diag: DiagnosticVec,
}

impl SourceLinter {
    fn new() -> Self {
        Self {
            diag: EcoVec::new(),
        }
    }

    fn lint(self, node: &SyntaxNode) -> DiagnosticVec {
        self.node(node);

        self.diag
    }

    fn node(&self, node: &SyntaxNode) -> Option<()> {
        let _ = node;
        todo!()
    }
}
