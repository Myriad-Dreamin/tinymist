//! Dead code detection for Typst.

mod collector;
mod diagnostic;

use rustc_hash::{FxHashMap, FxHashSet};

use tinymist_analysis::{
    adt::interner::Interned,
    syntax::{Decl, Expr, ExprInfo, RefExpr},
};
use tinymist_project::LspWorld;
use typst::{
    ecow::EcoVec,
    syntax::{FileId, LinkedNode, ast},
};

use crate::DiagnosticVec;
use collector::{DefInfo, DefScope, collect_definitions};
use diagnostic::generate_diagnostic;

struct ImportUsageInfo {
    used: FxHashSet<Interned<Decl>>,
    shadowed: FxHashSet<Interned<Decl>>,
    module_children: FxHashMap<Interned<Decl>, FxHashSet<Interned<Decl>>>,
    module_used_decls: FxHashSet<Interned<Decl>>,
}

/// Configuration for dead code detection.
#[derive(Debug, Clone, Hash)]
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
    cross_file_refs: &FxHashSet<Interned<Decl>>,
    config: &DeadCodeConfig,
) -> DiagnosticVec {
    let mut diagnostics = EcoVec::new();

    let definitions = collect_definitions(ei);

    if definitions.is_empty() {
        return diagnostics;
    }

    let ImportUsageInfo {
        used,
        shadowed,
        module_children,
        module_used_decls,
    } = compute_import_usage(&definitions, ei);

    let mut seen_decls: FxHashSet<Interned<Decl>> = FxHashSet::default();

    let has_references = |decl: &Interned<Decl>| -> bool {
        if matches!(decl.as_ref(), Decl::PathStem(_)) {
            // Path stems are "used" when they appear as an intermediate step in
            // resolution (e.g. when building module import graphs).
            return ei.resolves.values().any(|r| {
                matches!(
                    r.step.as_ref(),
                    Some(Expr::Decl(step_decl)) if step_decl == decl
                )
            });
        }

        if ei
            .get_refs(decl.clone())
            .any(|(_, r)| r.as_ref().decl != *decl)
        {
            return true;
        }

        cross_file_refs.contains(decl)
    };

    for def_info in definitions {
        let def_info = if config.check_exported
            && matches!(def_info.scope, DefScope::File)
            && is_exported_symbol_candidate(def_info.decl.as_ref())
            && ei.is_exported(&def_info.decl)
        {
            DefInfo {
                scope: DefScope::Exported,
                ..def_info
            }
        } else {
            def_info
        };

        if shadowed.contains(&def_info.decl) {
            continue;
        }
        if should_skip_definition(&def_info, config) {
            continue;
        }
        if !seen_decls.insert(def_info.decl.clone()) {
            continue;
        }

        let is_unused = match def_info.decl.as_ref() {
            Decl::Import(_) | Decl::ImportAlias(_) => !used.contains(&def_info.decl),
            Decl::ModuleImport(_) | Decl::ModuleAlias(_) => {
                let decl_used = used.contains(&def_info.decl);
                let children_used = module_children
                    .get(&def_info.decl)
                    .is_some_and(|children| children.iter().any(|child| used.contains(child)));
                let module_used = module_used_decls.contains(&def_info.decl);
                !(children_used || module_used || decl_used || has_references(&def_info.decl))
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

fn compute_import_usage(definitions: &[DefInfo], ei: &ExprInfo) -> ImportUsageInfo {
    let mut alias_links: FxHashMap<Interned<Decl>, Interned<Decl>> = FxHashMap::default();
    let mut shadowed: FxHashSet<Interned<Decl>> = FxHashSet::default();
    let mut module_children: FxHashMap<Interned<Decl>, FxHashSet<Interned<Decl>>> =
        FxHashMap::default();
    let mut module_targets: FxHashMap<Interned<Decl>, FileId> = FxHashMap::default();

    for (child, layout) in ei.module_items.iter() {
        module_children
            .entry(layout.parent.clone())
            .or_default()
            .insert(child.clone());
    }

    for def in definitions {
        if matches!(
            def.decl.as_ref(),
            Decl::ModuleImport(_) | Decl::ModuleAlias(_)
        ) {
            if let Some(r) = ei.resolves.get(&def.span) {
                let fid = r
                    .root
                    .as_ref()
                    .and_then(|expr| expr.file_id())
                    .or_else(|| r.step.as_ref().and_then(|expr| expr.file_id()));
                if let Some(fid) = fid {
                    module_targets.insert(def.decl.clone(), fid);
                }
            }
        }

        if matches!(def.decl.as_ref(), Decl::ImportAlias(_)) {
            if let Some(alias_ref) = ei.resolves.get(&def.span) {
                if let Some(Expr::Decl(step_decl)) = alias_ref.step.as_ref() {
                    alias_links.insert(def.decl.clone(), step_decl.clone());
                    shadowed.insert(step_decl.clone());
                }
            }
        }
    }

    let mut used: FxHashSet<Interned<Decl>> = FxHashSet::default();

    for r in ei.resolves.values() {
        if matches!(r.decl.as_ref(), Decl::IdentRef(_)) {
            collect_used_decls(r.as_ref(), &mut used);
        }
    }

    let mut worklist: Vec<_> = used
        .iter()
        .filter(|decl| alias_links.contains_key(decl))
        .cloned()
        .collect();

    while let Some(alias) = worklist.pop() {
        if let Some(target) = alias_links.get(&alias) {
            if used.insert(target.clone()) {
                worklist.push(target.clone());
            }
        }
    }

    let mut used_module_files: FxHashSet<FileId> = FxHashSet::default();
    for decl in &used {
        if let Some(fid) = decl.file_id() {
            used_module_files.insert(fid);
        }
    }

    let mut module_used_decls: FxHashSet<Interned<Decl>> = FxHashSet::default();
    let mut module_used_candidates: FxHashMap<FileId, Vec<Interned<Decl>>> = FxHashMap::default();
    for (decl, fid) in &module_targets {
        if module_children.contains_key(decl) {
            continue;
        }

        let is_candidate = match decl.as_ref() {
            Decl::ModuleImport(_) => true,
            Decl::ModuleAlias(_) => is_wildcard_module_import_decl(ei, decl),
            _ => false,
        };
        if !is_candidate {
            continue;
        }

        module_used_candidates
            .entry(*fid)
            .or_default()
            .push(decl.clone());
    }

    for (fid, decls) in module_used_candidates {
        if !used_module_files.contains(&fid) {
            continue;
        }

        let Some(chosen) = decls.into_iter().min_by_key(|decl| decl.span().into_raw()) else {
            continue;
        };
        module_used_decls.insert(chosen);
    }

    ImportUsageInfo {
        used,
        shadowed,
        module_children,
        module_used_decls,
    }
}

fn is_exported_symbol_candidate(decl: &Decl) -> bool {
    matches!(
        decl,
        Decl::Func(_) | Decl::Var(_) | Decl::Module(_) | Decl::Closure(_)
    )
}

fn is_wildcard_module_import_decl(ei: &ExprInfo, decl: &Interned<Decl>) -> bool {
    let span = decl.span();
    if span.is_detached() {
        return false;
    }

    let node = LinkedNode::new(ei.source.root()).find(span);
    let mut current = node;
    while let Some(node) = current {
        if let Some(module_import) = node.cast::<ast::ModuleImport>() {
            return matches!(module_import.imports(), Some(ast::Imports::Wildcard));
        }
        current = node.parent().cloned();
    }

    false
}

fn collect_used_decls(reference: &RefExpr, used: &mut FxHashSet<Interned<Decl>>) {
    let mut visited_refs: FxHashSet<Interned<RefExpr>> = FxHashSet::default();
    let mut worklist = Vec::new();

    if let Some(step) = reference.step.as_ref() {
        worklist.push(step);
    }
    if let Some(root) = reference.root.as_ref() {
        worklist.push(root);
    }

    while let Some(expr) = worklist.pop() {
        match expr {
            Expr::Decl(decl) => {
                used.insert(decl.clone());
            }
            Expr::Ref(reference) => {
                if visited_refs.insert(reference.clone()) {
                    if let Some(step) = reference.step.as_ref() {
                        worklist.push(step);
                    }
                    if let Some(root) = reference.root.as_ref() {
                        worklist.push(root);
                    }
                }
            }
            Expr::Select(select) => {
                worklist.push(&select.lhs);
            }
            _ => {}
        }
    }
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

    #[test]
    fn collect_used_decls_marks_root_and_step() {
        use tinymist_analysis::adt::interner::Interned;

        let module_decl = Interned::new(Decl::lit_(Interned::new_str("module")));
        let import_decl = Interned::new(Decl::lit_(Interned::new_str("imported")));

        let reference = RefExpr {
            decl: import_decl.clone(),
            step: Some(Expr::Decl(import_decl.clone())),
            root: Some(Expr::Decl(module_decl.clone())),
            term: None,
        };

        let mut used: FxHashSet<Interned<Decl>> = FxHashSet::default();
        collect_used_decls(&reference, &mut used);

        assert!(used.contains(&module_decl));
        assert!(used.contains(&import_decl));
    }
}
