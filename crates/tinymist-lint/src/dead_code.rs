//! Dead code detection for Typst.

mod collector;
mod diagnostic;

use std::collections::{HashMap, HashSet};

use tinymist_analysis::{
    adt::interner::Interned,
    syntax::{Decl, Expr, ExprInfo, RefExpr},
};
use tinymist_project::LspWorld;
use typst::{ecow::EcoVec, syntax::FileId};

use crate::DiagnosticVec;
use collector::{DefInfo, DefScope, collect_definitions};
use diagnostic::generate_diagnostic;

struct ImportUsageInfo {
    used: HashSet<Interned<Decl>>,
    shadowed: HashSet<Interned<Decl>>,
    module_children: HashMap<Interned<Decl>, HashSet<Interned<Decl>>>,
    module_targets: HashMap<Interned<Decl>, FileId>,
    used_module_files: HashSet<FileId>,
}

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

    let ImportUsageInfo {
        used,
        shadowed,
        module_children,
        module_targets,
        used_module_files,
    } = compute_import_usage(&definitions, ei);

    let mut seen_module_aliases = HashSet::new();

    for def_info in definitions {
        if matches!(def_info.decl.as_ref(), Decl::ModuleAlias(_))
            && !seen_module_aliases.insert(def_info.decl.clone())
        {
            continue;
        }
        if shadowed.contains(&def_info.decl) {
            continue;
        }
        if should_skip_definition(&def_info, config) {
            continue;
        }

        let is_unused = match def_info.decl.as_ref() {
            Decl::Import(_) | Decl::ImportAlias(_) => !used.contains(&def_info.decl),
            Decl::ModuleImport(_) | Decl::ModuleAlias(_) => {
                let children_used = module_children
                    .get(&def_info.decl)
                    .is_some_and(|children| children.iter().any(|child| used.contains(child)));
                let module_used = if module_children.contains_key(&def_info.decl) {
                    false
                } else {
                    module_targets
                        .get(&def_info.decl)
                        .is_some_and(|fid| used_module_files.contains(fid))
                };
                !(children_used || module_used || has_references(&def_info.decl))
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
    let mut alias_links: HashMap<Interned<Decl>, Interned<Decl>> = HashMap::new();
    let mut shadowed = HashSet::new();
    let mut module_children: HashMap<Interned<Decl>, HashSet<Interned<Decl>>> = HashMap::new();
    let mut module_targets: HashMap<Interned<Decl>, FileId> = HashMap::new();

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

    let mut used: HashSet<Interned<Decl>> = HashSet::new();

    for r in ei.resolves.values() {
        if matches!(r.decl.as_ref(), Decl::IdentRef(_)) {
            collect_used_decls(r.as_ref(), &mut used);
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

    let mut used_module_files = HashSet::new();
    for decl in &used {
        if let Some(fid) = decl.file_id() {
            used_module_files.insert(fid);
        }
    }

    ImportUsageInfo {
        used,
        shadowed,
        module_children,
        module_targets,
        used_module_files,
    }
}

fn collect_used_decls(reference: &RefExpr, used: &mut HashSet<Interned<Decl>>) {
    let mut visited_refs = HashSet::new();
    if let Some(step) = reference.step.as_ref() {
        collect_decl_from_expr(step, used, &mut visited_refs);
    }
    if let Some(root) = reference.root.as_ref() {
        collect_decl_from_expr(root, used, &mut visited_refs);
    }
}

fn collect_decl_from_expr(
    expr: &Expr,
    used: &mut HashSet<Interned<Decl>>,
    visited_refs: &mut HashSet<Interned<RefExpr>>,
) {
    match expr {
        Expr::Decl(decl) => {
            used.insert(decl.clone());
        }
        Expr::Ref(reference) => {
            if visited_refs.insert(reference.clone()) {
                if let Some(step) = reference.step.as_ref() {
                    collect_decl_from_expr(step, used, visited_refs);
                }
                if let Some(root) = reference.root.as_ref() {
                    collect_decl_from_expr(root, used, visited_refs);
                }
            }
        }
        Expr::Select(select) => {
            collect_decl_from_expr(&select.lhs, used, visited_refs);
        }
        _ => {}
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
    use std::collections::HashSet;

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

        let mut used = HashSet::new();
        collect_used_decls(&reference, &mut used);

        assert!(used.contains(&module_decl));
        assert!(used.contains(&import_decl));
    }
}
