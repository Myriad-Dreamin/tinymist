//! Dead code detection for Typst.

mod collector;
mod diagnostic;

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use tinymist_analysis::{
    adt::interner::Interned,
    syntax::{Decl, Expr, ExprInfo},
};
use tinymist_project::{LspWorld, SourceWorld};
use typst::{ecow::EcoVec, syntax::FileId};

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

    let (import_usage, shadowed_imports, module_children) =
        compute_import_usage(world, &definitions, ei);

    let mut seen_module_aliases = HashSet::new();

    for def_info in definitions {
        if matches!(def_info.decl.as_ref(), Decl::ModuleAlias(_))
            && !seen_module_aliases.insert(def_info.decl.clone())
        {
            continue;
        }
        if shadowed_imports.contains(&def_info.decl) {
            continue;
        }
        if should_skip_definition(&def_info, config) {
            continue;
        }

        let is_unused = match def_info.decl.as_ref() {
            Decl::Import(_) | Decl::ImportAlias(_) => !import_usage.contains(&def_info.decl),
            Decl::ModuleImport(_) | Decl::ModuleAlias(_) => {
                let children_used = module_children.get(&def_info.decl).is_some_and(|children| {
                    children.iter().any(|child| import_usage.contains(child))
                });
                !(children_used || has_references(&def_info.decl))
            }
            _ => !has_references(&def_info.decl),
        };

        if is_unused {
            if let Some(diag) = generate_diagnostic(&def_info, world, ei) {
                diagnostics.push(diag);
            }
        }
    }

    diagnostics
}

fn compute_import_usage(
    world: &LspWorld,
    definitions: &[DefInfo],
    ei: &ExprInfo,
) -> (
    HashSet<Interned<Decl>>,
    HashSet<Interned<Decl>>,
    HashMap<Interned<Decl>, HashSet<Interned<Decl>>>,
) {
    struct ModuleSpan {
        decl: Interned<Decl>,
        fid: FileId,
        range: Range<usize>,
    }

    let text = ei.source.text();
    let module_spans: Vec<ModuleSpan> = definitions
        .iter()
        .filter_map(|def| match def.decl.as_ref() {
            Decl::ModuleImport(_) => world.source_range(def.span).map(|range| ModuleSpan {
                decl: def.decl.clone(),
                fid: def.fid,
                range,
            }),
            _ => None,
        })
        .collect();

    let mut alias_links: HashMap<Interned<Decl>, Interned<Decl>> = HashMap::new();
    let mut shadowed = HashSet::new();
    let mut module_children: HashMap<Interned<Decl>, HashSet<Interned<Decl>>> = HashMap::new();
    let mut alias_item_ranges: Vec<(Range<usize>, Interned<Decl>)> = Vec::new();

    for def in definitions {
        if matches!(def.decl.as_ref(), Decl::ImportAlias(_)) {
            if let Some(alias_ref) = ei.resolves.get(&def.span) {
                if let Some(Expr::Decl(step_decl)) = alias_ref.step.as_ref() {
                    alias_links.insert(def.decl.clone(), step_decl.clone());
                    shadowed.insert(step_decl.clone());
                }
            }
        }
        if matches!(def.decl.as_ref(), Decl::ModuleAlias(_)) {
            if let Some(alias_range) = world.source_range(def.span) {
                if let Some(items_range) = alias_items_range(text, &alias_range) {
                    alias_item_ranges.push((items_range, def.decl.clone()));
                }
            }
        }
    }

    for def in definitions {
        if matches!(def.decl.as_ref(), Decl::Import(_) | Decl::ImportAlias(_)) {
            if let Some(child_range) = world.source_range(def.span) {
                if let Some(module) = module_spans
                    .iter()
                    .find(|span| span.fid == def.fid && contains_range(&span.range, &child_range))
                {
                    module_children
                        .entry(module.decl.clone())
                        .or_default()
                        .insert(def.decl.clone());
                }

                if let Some((_, alias_decl)) = alias_item_ranges
                    .iter()
                    .find(|(range, _)| range.contains(&child_range.start))
                {
                    module_children
                        .entry(alias_decl.clone())
                        .or_default()
                        .insert(def.decl.clone());
                }
            }
        }
    }

    let mut used: HashSet<Interned<Decl>> = HashSet::new();

    for r in ei.resolves.values() {
        if matches!(r.decl.as_ref(), Decl::IdentRef(_)) {
            if let Some(Expr::Decl(step_decl)) = r.step.as_ref() {
                used.insert(step_decl.clone());
            }
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for (alias, target) in alias_links.iter() {
            if used.contains(alias) && !used.contains(target) {
                used.insert(target.clone());
                changed = true;
            }
        }
    }

    (used, shadowed, module_children)
}

fn contains_range(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start && outer.end >= inner.end
}

fn alias_items_range(text: &str, alias_range: &Range<usize>) -> Option<Range<usize>> {
    let bytes = text.as_bytes();
    let mut idx = alias_range.end;
    while idx < bytes.len() && matches!(bytes[idx], b' ' | b'\t') {
        idx += 1;
    }
    if idx >= bytes.len() || bytes[idx] != b':' {
        return None;
    }
    idx += 1;
    while idx < bytes.len() && matches!(bytes[idx], b' ' | b'\t') {
        idx += 1;
    }
    let mut end = idx;
    while end < bytes.len() && bytes[end] != b'\n' && bytes[end] != b'\r' {
        end += 1;
    }
    Some(idx..end)
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
        Decl::Pattern(_) | Decl::Spread(_) | Decl::Constant(_) | Decl::Content(_)
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

    if let Some(suffix) = pattern.strip_prefix('*') {
        return name.ends_with(suffix);
    }

    if let Some(prefix) = pattern.strip_suffix('*') {
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
