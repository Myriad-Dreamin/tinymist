//! Dead code detection for Typst.

mod collector;
mod diagnostic;

use tinymist_analysis::{
    adt::interner::Interned,
    syntax::{Decl, ExprInfo},
};
use tinymist_project::LspWorld;
use typst::ecow::EcoVec;

use crate::DiagnosticVec;
use collector::{DefInfo, DefScope, collect_definitions};
use diagnostic::generate_diagnostic;

/// Configuration for dead code detection.
#[derive(Debug, Clone)]
pub struct DeadCodeConfig {
    /// Whether to check exported but unused symbols.
    pub check_exported: bool,
    /// Whether to check unused function parameters.
    pub check_params: bool,
    /// Patterns for exceptions (e.g., "test_*", "_*").
    pub exceptions: Vec<String>,
}

impl Default for DeadCodeConfig {
    fn default() -> Self {
        Self {
            check_exported: false,
            check_params: true,
            exceptions: vec!["_*".to_string()],
        }
    }
}

pub fn check_dead_code(
    world: &LspWorld,
    ei: &ExprInfo,
    has_references: impl Fn(&Interned<Decl>) -> bool,
    config: &DeadCodeConfig,
) -> DiagnosticVec {
    let mut diagnostics = EcoVec::new();

    let definitions = collect_definitions(ei);

    if definitions.is_empty() {
        return diagnostics;
    }

    for def_info in definitions {
        if should_skip_definition(&def_info, config) {
            continue;
        }

        if !has_references(&def_info.decl) {
            if let Some(diag) = generate_diagnostic(&def_info, world, ei) {
                diagnostics.push(diag);
            }
        }
    }

    diagnostics
}

fn should_skip_definition(def_info: &DefInfo, config: &DeadCodeConfig) -> bool {
    if matches!(def_info.scope, DefScope::Exported) && !config.check_exported {
        return true;
    }
    if matches!(def_info.scope, DefScope::FunctionParam) && !config.check_params {
        return true;
    }
    if matches!(def_info.decl.as_ref(), Decl::Generated(_)) {
        return true;
    }

    let name = def_info.decl.name().as_ref();
    for pattern in &config.exceptions {
        if matches_pattern(name, pattern) {
            return true;
        }
    }

    matches!(
        def_info.decl.as_ref(),
        Decl::ModuleImport(_)
            | Decl::Pattern(_)
            | Decl::Spread(_)
            | Decl::Constant(_)
            | Decl::Content(_)
    )
}

fn matches_pattern(name: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if pattern.starts_with('*') && pattern.ends_with('*') {
        let middle = &pattern[1..pattern.len() - 1];
        return name.contains(middle);
    }

    if pattern.starts_with('*') {
        let suffix = &pattern[1..];
        return name.ends_with(suffix);
    }

    if pattern.ends_with('*') {
        let prefix = &pattern[..pattern.len() - 1];
        return name.starts_with(prefix);
    }

    name == pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matching() {
        assert!(matches_pattern("_unused", "_*"));
        assert!(matches_pattern("test_foo", "test_*"));
        assert!(matches_pattern("my_test", "*_test"));
        assert!(matches_pattern("foo_test_bar", "*_test_*"));
        assert!(!matches_pattern("foo", "bar"));
    }
}
