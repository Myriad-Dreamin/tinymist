//! Dependency-only syntax scanning.
//!
//! This module deliberately does not perform expression analysis. It only
//! interprets the small, side-effect-free subset of Typst expressions that is
//! useful for discovering possible import and include paths.

use ecow::{EcoString, EcoVec};
use rustc_hash::{FxHashMap, FxHashSet};
use typst::syntax::{
    Source, Span, SyntaxKind, SyntaxNode,
    ast::{self, AstNode},
};

/// The kind of syntax that introduced a dependency site.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub(crate) enum DependencySiteKind {
    /// An `import` expression.
    Import,
    /// An `include` expression.
    Include,
}

/// The dependency-only interpretation of an import or include source.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) enum DependencyTarget {
    /// All statically possible source strings.
    ///
    /// The vector is non-empty and contains no duplicates. Multiple entries
    /// conservatively represent control-flow alternatives.
    Exact(EcoVec<EcoString>),
    /// The source may depend on syntax outside the supported static subset.
    ///
    /// Callers must retain this site. It does not mean that the expression has
    /// no dependency.
    UnknownDynamic,
}

/// One syntactic import or include site in a source file.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub(crate) struct DependencySite {
    /// Whether this site is an import or include.
    pub(crate) kind: DependencySiteKind,
    /// The span of the import/include's source expression.
    pub(crate) source_span: Span,
    /// The statically known source candidates, or an explicit dynamic marker.
    pub(crate) target: DependencyTarget,
}

/// Scans every import and include site in a source's complete syntax tree.
///
/// In addition to top-level sites, this visits nested content, code blocks,
/// closure bodies, conditional branches, and loop bodies. Static source
/// evaluation intentionally supports only:
///
/// - string literals;
/// - parenthesized expressions;
/// - references to immutable, simple `let` bindings;
/// - string concatenation with `+`;
/// - conservative alternatives from `if`/`else` when both branches are
///   statically known.
///
/// Everything else is reported as [`DependencyTarget::UnknownDynamic`].
pub(crate) fn scan_dependency_sites(source: &Source) -> EcoVec<DependencySite> {
    scan_dependency_root(source.root())
}

/// Separates the completeness-critical raw syntax walk from best-effort static
/// evaluation. A specialized scope walker can conservatively miss an AST form,
/// especially in an error-recovery tree; the raw walk remains authoritative for
/// whether an import/include site exists.
fn scan_dependency_root(root: &SyntaxNode) -> EcoVec<DependencySite> {
    let resolved = DependencyScanner::new().scan(root);
    let mut sites = EcoVec::new();
    collect_raw_dependency_sites(root, &resolved, &mut sites);
    sites
}

fn collect_raw_dependency_sites(
    node: &SyntaxNode,
    resolved: &ResolvedSites,
    sites: &mut EcoVec<DependencySite>,
) {
    let site = match node.kind() {
        SyntaxKind::ModuleImport => node
            .cast::<ast::ModuleImport<'_>>()
            .map(|import| (DependencySiteKind::Import, import.source().span())),
        SyntaxKind::ModuleInclude => node
            .cast::<ast::ModuleInclude<'_>>()
            .map(|include| (DependencySiteKind::Include, include.source().span())),
        _ => None,
    };

    if let Some((kind, source_span)) = site {
        sites.push(DependencySite {
            kind,
            source_span,
            target: resolved
                .get(&(kind, source_span))
                .cloned()
                .unwrap_or(DependencyTarget::UnknownDynamic),
        });
    }

    for child in node.children() {
        collect_raw_dependency_sites(child, resolved, sites);
    }
}

type StaticEnv = FxHashMap<EcoString, DependencyTarget>;
type ResolvedSites = FxHashMap<(DependencySiteKind, Span), DependencyTarget>;

/// Prevent pathological cross products from consuming unbounded memory. Once
/// the exact set grows beyond this limit, `UnknownDynamic` is the conservative
/// result: no possible dependency is silently discarded.
const MAX_EXACT_CANDIDATES: usize = 256;

struct DependencyScanner {
    resolved: ResolvedSites,
}

impl DependencyScanner {
    fn new() -> Self {
        Self {
            resolved: FxHashMap::default(),
        }
    }

    fn scan(mut self, root: &SyntaxNode) -> ResolvedSites {
        self.walk_node(root, &mut StaticEnv::default());
        self.resolved
    }

    fn walk_node(&mut self, node: &SyntaxNode, env: &mut StaticEnv) {
        if let Some(expr) = node.cast::<ast::Expr<'_>>() {
            self.walk_expr(expr, env);
            return;
        }

        for child in node.children() {
            self.walk_node(child, env);
        }
    }

    fn walk_expr(&mut self, expr: ast::Expr<'_>, env: &mut StaticEnv) {
        match expr {
            ast::Expr::ModuleImport(import) => self.walk_import(import, env),
            ast::Expr::ModuleInclude(include) => {
                let source = include.source();
                self.record(DependencySiteKind::Include, source, env);
                self.walk_expr(source, env);
            }
            ast::Expr::LetBinding(binding) => self.walk_let(binding, env),
            ast::Expr::Closure(closure) => self.walk_closure(closure, env),
            ast::Expr::CodeBlock(block) => {
                let mut nested = env.clone();
                for expr in block.body().exprs() {
                    self.walk_expr(expr, &mut nested);
                }
            }
            ast::Expr::ContentBlock(block) => {
                let mut nested = env.clone();
                for expr in block.body().exprs() {
                    self.walk_expr(expr, &mut nested);
                }
            }
            ast::Expr::Heading(_)
            | ast::Expr::ListItem(_)
            | ast::Expr::EnumItem(_)
            | ast::Expr::TermItem(_) => {
                // ExprWorker checks these markup bodies with `with_scope`.
                // Keep their bindings visible within the body, but do not let
                // them escape into the surrounding sequential scope.
                let mut nested = env.clone();
                for child in expr.to_untyped().children() {
                    self.walk_node(child, &mut nested);
                }
            }
            ast::Expr::Conditional(conditional) => {
                self.walk_expr(conditional.condition(), env);
                self.walk_expr(conditional.if_body(), env);
                if let Some(otherwise) = conditional.else_body() {
                    self.walk_expr(otherwise, env);
                }
            }
            ast::Expr::WhileLoop(while_loop) => {
                self.walk_expr(while_loop.condition(), env);
                self.walk_expr(while_loop.body(), env);
            }
            ast::Expr::ForLoop(for_loop) => {
                // ExprWorker checks the whole loop in one nested scope, with
                // the pattern before the iterable and body.
                let mut loop_env = env.clone();
                self.walk_pattern(for_loop.pattern(), &mut loop_env);
                self.walk_expr(for_loop.iterable(), &mut loop_env);
                self.walk_expr(for_loop.body(), &mut loop_env);
            }
            ast::Expr::Binary(binary)
                if matches!(
                    binary.op(),
                    ast::BinOp::Assign
                        | ast::BinOp::AddAssign
                        | ast::BinOp::SubAssign
                        | ast::BinOp::MulAssign
                        | ast::BinOp::DivAssign
                ) =>
            {
                self.walk_assignment(binary, env);
            }
            ast::Expr::DestructAssignment(assignment) => {
                self.walk_expr(assignment.value(), env);
                mask_pattern(env, assignment.pattern());
            }
            _ => {
                // ExprWorker only introduces lexical scopes for the explicit
                // forms handled above. All other expressions (including
                // strong/emphasis, math, arrays, dictionaries, and calls)
                // share their surrounding lexical context, so bindings must
                // remain visible to later siblings and expressions here too.
                for child in expr.to_untyped().children() {
                    self.walk_node(child, env);
                }
            }
        }
    }

    fn walk_assignment(&mut self, binary: ast::Binary<'_>, env: &mut StaticEnv) {
        let lhs = binary.lhs();
        let rhs = binary.rhs();

        // Sites in either operand are still discovered by the raw pass. Walking
        // them here gives the static evaluator a chance to resolve their target.
        self.walk_expr(lhs, env);
        self.walk_expr(rhs, env);

        let Some(name) = assignment_name(lhs) else {
            return;
        };
        let rhs = self.eval_static_string(rhs, env);
        let value = match binary.op() {
            ast::BinOp::Assign => rhs,
            ast::BinOp::AddAssign => concat_targets(
                env.get(name.as_str())
                    .cloned()
                    .unwrap_or(DependencyTarget::UnknownDynamic),
                rhs,
            ),
            ast::BinOp::SubAssign | ast::BinOp::MulAssign | ast::BinOp::DivAssign => {
                DependencyTarget::UnknownDynamic
            }
            _ => unreachable!("walk_assignment only accepts assignment operators"),
        };
        env.insert(name, value);
    }

    fn walk_import(&mut self, import: ast::ModuleImport<'_>, env: &mut StaticEnv) {
        let source = import.source();
        self.record(DependencySiteKind::Import, source, env);
        self.walk_expr(source, env);

        match import.imports() {
            Some(ast::Imports::Wildcard) => {
                // A wildcard can shadow any statically tracked name.
                env.clear();
            }
            Some(ast::Imports::Items(items)) => {
                for item in items.iter() {
                    env.insert(
                        item.bound_name().get().clone(),
                        DependencyTarget::UnknownDynamic,
                    );
                }
            }
            None => {
                if let Some(name) = import.new_name() {
                    env.insert(name.get().clone(), DependencyTarget::UnknownDynamic);
                } else if let Ok(name) = import.bare_name() {
                    env.insert(name, DependencyTarget::UnknownDynamic);
                }
            }
        }
    }

    fn walk_let(&mut self, binding: ast::LetBinding<'_>, env: &mut StaticEnv) {
        if let Some(init) = binding.init() {
            self.walk_expr(init, env);
        }

        let ast::LetBindingKind::Normal(pattern) = binding.kind() else {
            for name in binding.kind().bindings() {
                env.insert(name.get().clone(), DependencyTarget::UnknownDynamic);
            }
            return;
        };

        let Some(name) = simple_pattern_name(pattern) else {
            mask_pattern(env, pattern);
            return;
        };

        let value = binding
            .init()
            .map(|init| self.eval_static_string(init, env))
            .unwrap_or(DependencyTarget::UnknownDynamic);
        env.insert(name.get().clone(), value);
    }

    fn walk_closure(&mut self, closure: ast::Closure<'_>, env: &StaticEnv) {
        let mut closure_env = env.clone();
        if let Some(name) = closure.name() {
            closure_env.insert(name.get().clone(), DependencyTarget::UnknownDynamic);
        }

        // Defaults can refer to earlier parameters, so mask each binding after
        // scanning its default and before proceeding to the next parameter.
        for param in closure.params().children() {
            match param {
                ast::Param::Pos(pattern) => self.walk_pattern(pattern, &mut closure_env),
                ast::Param::Named(named) => {
                    self.walk_expr(named.expr(), &mut closure_env);
                    closure_env
                        .insert(named.name().get().clone(), DependencyTarget::UnknownDynamic);
                }
                ast::Param::Spread(spread) => {
                    self.walk_expr(spread.expr(), &mut closure_env);
                    if let Some(name) = spread.sink_ident() {
                        closure_env.insert(name.get().clone(), DependencyTarget::UnknownDynamic);
                    }
                }
            }
        }

        self.walk_expr(closure.body(), &mut closure_env);
    }

    fn walk_pattern(&mut self, pattern: ast::Pattern<'_>, env: &mut StaticEnv) {
        // Valid binding patterns mask their names. Walking the raw children as
        // expressions also keeps dependency targets for permissive/error-
        // recovery syntax aligned with ExprWorker::check_pattern.
        for child in pattern.to_untyped().children() {
            self.walk_node(child, env);
        }
        mask_pattern(env, pattern);
    }

    fn record(&mut self, kind: DependencySiteKind, source: ast::Expr<'_>, env: &StaticEnv) {
        self.resolved
            .insert((kind, source.span()), self.eval_static_string(source, env));
    }

    fn eval_static_string(&self, expr: ast::Expr<'_>, env: &StaticEnv) -> DependencyTarget {
        match expr {
            ast::Expr::Str(string) => exact_one(string.get()),
            ast::Expr::Parenthesized(parenthesized) => {
                self.eval_static_string(parenthesized.expr(), env)
            }
            ast::Expr::Ident(ident) => env
                .get(ident.as_str())
                .cloned()
                .unwrap_or(DependencyTarget::UnknownDynamic),
            ast::Expr::Binary(binary) if binary.op() == ast::BinOp::Add => concat_targets(
                self.eval_static_string(binary.lhs(), env),
                self.eval_static_string(binary.rhs(), env),
            ),
            ast::Expr::Conditional(conditional) => {
                let Some(otherwise) = conditional.else_body() else {
                    return DependencyTarget::UnknownDynamic;
                };
                union_targets(
                    self.eval_static_string(conditional.if_body(), env),
                    self.eval_static_string(otherwise, env),
                )
            }
            ast::Expr::CodeBlock(block) => self.eval_static_block(block, env),
            _ => DependencyTarget::UnknownDynamic,
        }
    }

    fn eval_static_block(&self, block: ast::CodeBlock<'_>, env: &StaticEnv) -> DependencyTarget {
        let mut env = env.clone();
        let exprs: Vec<_> = block.body().exprs().collect();
        let Some((last, prefix)) = exprs.split_last() else {
            return DependencyTarget::UnknownDynamic;
        };

        for expr in prefix {
            let ast::Expr::LetBinding(binding) = *expr else {
                return DependencyTarget::UnknownDynamic;
            };
            self.bind_static_let(binding, &mut env);
        }

        self.eval_static_string(*last, &env)
    }

    fn bind_static_let(&self, binding: ast::LetBinding<'_>, env: &mut StaticEnv) {
        let ast::LetBindingKind::Normal(pattern) = binding.kind() else {
            for name in binding.kind().bindings() {
                env.insert(name.get().clone(), DependencyTarget::UnknownDynamic);
            }
            return;
        };
        let Some(name) = simple_pattern_name(pattern) else {
            mask_pattern(env, pattern);
            return;
        };

        let value = binding
            .init()
            .map(|init| self.eval_static_string(init, env))
            .unwrap_or(DependencyTarget::UnknownDynamic);
        env.insert(name.get().clone(), value);
    }
}

fn exact_one(value: EcoString) -> DependencyTarget {
    let mut values = EcoVec::new();
    values.push(value);
    DependencyTarget::Exact(values)
}

fn concat_targets(left: DependencyTarget, right: DependencyTarget) -> DependencyTarget {
    let (DependencyTarget::Exact(left), DependencyTarget::Exact(right)) = (left, right) else {
        return DependencyTarget::UnknownDynamic;
    };

    let Some(product) = left.len().checked_mul(right.len()) else {
        return DependencyTarget::UnknownDynamic;
    };
    if product == 0 || product > MAX_EXACT_CANDIDATES {
        return DependencyTarget::UnknownDynamic;
    }

    let mut seen = FxHashSet::default();
    let mut values = EcoVec::new();
    for lhs in &left {
        for rhs in &right {
            let mut value = EcoString::with_capacity(lhs.len() + rhs.len());
            value.push_str(lhs);
            value.push_str(rhs);
            if seen.insert(value.clone()) {
                values.push(value);
            }
        }
    }
    DependencyTarget::Exact(values)
}

fn union_targets(left: DependencyTarget, right: DependencyTarget) -> DependencyTarget {
    let (DependencyTarget::Exact(left), DependencyTarget::Exact(right)) = (left, right) else {
        return DependencyTarget::UnknownDynamic;
    };
    if left.len().saturating_add(right.len()) > MAX_EXACT_CANDIDATES {
        return DependencyTarget::UnknownDynamic;
    }

    let mut seen = FxHashSet::default();
    let mut values = EcoVec::new();
    for value in left.into_iter().chain(right) {
        if seen.insert(value.clone()) {
            values.push(value);
        }
    }
    if values.is_empty() {
        DependencyTarget::UnknownDynamic
    } else {
        DependencyTarget::Exact(values)
    }
}

fn simple_pattern_name(pattern: ast::Pattern<'_>) -> Option<ast::Ident<'_>> {
    match pattern {
        ast::Pattern::Normal(ast::Expr::Ident(ident)) => Some(ident),
        ast::Pattern::Parenthesized(parenthesized) => simple_pattern_name(parenthesized.pattern()),
        _ => None,
    }
}

fn assignment_name(expr: ast::Expr<'_>) -> Option<EcoString> {
    match expr {
        ast::Expr::Ident(ident) => Some(ident.get().clone()),
        ast::Expr::Parenthesized(parenthesized) => assignment_name(parenthesized.expr()),
        _ => None,
    }
}

fn mask_pattern(env: &mut StaticEnv, pattern: ast::Pattern<'_>) {
    for name in pattern.bindings() {
        env.insert(name.get().clone(), DependencyTarget::UnknownDynamic);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exact(target: &DependencyTarget) -> Vec<&str> {
        let DependencyTarget::Exact(paths) = target else {
            panic!("expected exact dependency target, got {target:?}");
        };
        paths.iter().map(EcoString::as_str).collect()
    }

    #[test]
    fn scans_dormant_function_imports_with_static_strings() {
        let source = Source::detached(
            r#"
#let stem = "dep"
#let dormant() = {
  import (stem + ".typ")
  include "other.typ"
}
"#,
        );

        let sites = scan_dependency_sites(&source);
        assert_eq!(sites.len(), 2);
        assert_eq!(sites[0].kind, DependencySiteKind::Import);
        assert_eq!(exact(&sites[0].target), vec!["dep.typ"]);
        assert_eq!(sites[1].kind, DependencySiteKind::Include);
        assert_eq!(exact(&sites[1].target), vec!["other.typ"]);
    }

    #[test]
    fn retains_unknown_dynamic_and_unions_static_branches() {
        let source = Source::detached(
            r#"
#let dynamic(path) = import path
#let choose(flag) = import (if flag { "a.typ" } else { "b.typ" })
"#,
        );

        let sites = scan_dependency_sites(&source);
        assert_eq!(sites.len(), 2);
        assert_eq!(sites[0].target, DependencyTarget::UnknownDynamic);
        assert_eq!(exact(&sites[1].target), vec!["a.typ", "b.typ"]);
    }

    #[test]
    fn inner_shadow_assignment_does_not_poison_outer_binding() {
        let source = Source::detached(
            r#"
#let path = "outer.typ"
#{
  let path = "inner"
  path += ".typ"
}
#let dormant() = import path
#{
  path = "updated"
  path += ".typ"
  import path
}
#let after_block() = import path
"#,
        );

        let sites = scan_dependency_sites(&source);
        assert_eq!(sites.len(), 3);
        assert_eq!(exact(&sites[0].target), vec!["outer.typ"]);
        assert_eq!(exact(&sites[1].target), vec!["updated.typ"]);
        assert_eq!(exact(&sites[2].target), vec!["outer.typ"]);
    }

    #[test]
    fn inline_markup_binding_remains_in_the_surrounding_scope() {
        let source = Source::detached(
            r#"
*#let path = "inline.typ";*
#let dormant() = import path
"#,
        );

        let sites = scan_dependency_sites(&source);
        assert_eq!(sites.len(), 1);
        assert_eq!(exact(&sites[0].target), vec!["inline.typ"]);
    }

    #[test]
    fn raw_site_walk_covers_error_recovery_pattern_children() {
        fn find(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxNode> {
            if node.kind() == kind {
                return Some(node.clone());
            }
            node.children().find_map(|child| find(child, kind))
        }

        let parsed = Source::detached(r#"#import "nested.typ""#);
        let import = find(parsed.root(), SyntaxKind::ModuleImport)
            .expect("fixture must contain a module import");

        // A malformed let can retain an import expression in its pattern while
        // exposing a different expression as its initializer. The scope walker
        // intentionally ignores that invalid pattern; the raw walk must still
        // retain the dependency site as dynamic instead of dropping it.
        let malformed_let = SyntaxNode::inner(
            SyntaxKind::LetBinding,
            vec![
                import,
                SyntaxNode::leaf(SyntaxKind::Eq, "="),
                SyntaxNode::leaf(SyntaxKind::Str, r#""value""#),
            ],
        );
        let root = SyntaxNode::inner(SyntaxKind::Markup, vec![malformed_let]);

        let sites = scan_dependency_root(&root);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].kind, DependencySiteKind::Import);
        assert_eq!(sites[0].target, DependencyTarget::UnknownDynamic);
    }
}
