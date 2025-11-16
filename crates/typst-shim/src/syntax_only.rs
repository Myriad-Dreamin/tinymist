use std::sync::atomic::{AtomicBool, Ordering};
use typst::{
    Document, World,
    diag::{SourceDiagnostic, SourceResult, Warned},
    ecow::eco_vec,
};
use typst_syntax::Span;

/// Global flag indicating whether syntax-only mode is enabled.
pub static SYNTAX_ONLY: AtomicBool = AtomicBool::new(false);

/// Check if syntax-only mode is enabled.
pub fn is_syntax_only() -> bool {
    SYNTAX_ONLY.load(Ordering::Acquire)
}

/// Compile the document if syntax-only mode is disabled; otherwise, return an
/// error.
pub fn compile_opt<D>(world: &dyn World) -> Warned<SourceResult<D>>
where
    D: Document,
{
    if is_syntax_only() {
        Warned {
            output: Err(eco_vec![SourceDiagnostic::error(
                Span::detached(),
                "Compilation is disabled in syntax-only mode.",
            )]),
            warnings: Default::default(),
        }
    } else {
        typst::compile::<D>(world)
    }
}
