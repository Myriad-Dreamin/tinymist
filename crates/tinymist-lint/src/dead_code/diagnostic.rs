//! Diagnostic generator for dead code warnings.
//!
//! This module creates user-friendly diagnostic messages for unused
//! definitions, with appropriate hints and severity levels.

use tinymist_analysis::syntax::{Decl, DefKind, ExprInfo};
use tinymist_project::LspWorld;
use typst::diag::{SourceDiagnostic, eco_format};

use super::collector::{DefInfo, DefScope};

/// Generates a diagnostic for an unused definition.
///
/// Creates a warning with contextual information about the unused symbol,
/// including its kind (function, variable, etc.) and helpful suggestions.
pub fn generate_diagnostic(
    def_info: &DefInfo,
    _world: &LspWorld,
    ei: &ExprInfo,
) -> Option<SourceDiagnostic> {
    // Skip if the span is detached (synthetic or generated code)
    if def_info.span.is_detached() {
        return None;
    }

    let is_module_import = matches!(def_info.decl.as_ref(), Decl::ModuleImport(..));
    let is_module_like = is_module_import || matches!(def_info.kind, DefKind::Module);

    let kind_str = match def_info.kind {
        DefKind::Function => "function",
        DefKind::Variable => "variable",
        DefKind::Constant => "constant",
        DefKind::Module => "module",
        DefKind::Struct => "struct",
        DefKind::Reference => return None, // Labels/refs are handled separately
    };

    let name = def_info.decl.name();

    // Don't warn about empty names (anonymous items)
    if name.is_empty() && !is_module_import {
        return None;
    }

    // Create the base diagnostic
    let mut diag = if is_module_import {
        SourceDiagnostic::warning(def_info.span, eco_format!("unused module import"))
    } else {
        SourceDiagnostic::warning(def_info.span, eco_format!("unused {kind_str}: `{name}`"))
    };

    // Add helpful hints based on the scope and kind
    match def_info.scope {
        DefScope::FunctionParam => {
            diag = diag.with_hint(eco_format!(
                "if this parameter is intentionally unused, prefix it with underscore: `_{name}`"
            ));
        }
        DefScope::File | DefScope::Local if !is_module_like => {
            diag = diag.with_hint(eco_format!(
                "consider removing this {kind_str} or prefixing with underscore: `_{name}`"
            ));
        }
        DefScope::File | DefScope::Local => {}
        DefScope::Exported => {
            diag = diag.with_hint(eco_format!("this {kind_str} is exported but never used"));
        }
    }

    // Add kind-specific hints
    match def_info.kind {
        DefKind::Function => {
            // Check if there's a docstring - documented functions might be intentional API
            if matches!(def_info.scope, DefScope::Exported)
                && ei.docstrings.contains_key(&def_info.decl)
            {
                // Reduce severity for documented functions (they might be public API)
                return None;
            }
        }
        _ => {}
    }

    if is_module_like {
        diag = diag.with_hint("imported modules should be used or the import should be removed");
    }

    Some(diag)
}
