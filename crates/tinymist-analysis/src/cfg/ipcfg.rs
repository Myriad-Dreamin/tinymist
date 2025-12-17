use rustc_hash::{FxHashMap, FxHashSet};
use typst::syntax::ast::AstNode;
use typst::syntax::{Span, SyntaxNode, ast};

use crate::syntax::{Expr, ExprInfo, RefExpr as AnalysisRefExpr};

use super::builder::build_cfgs_many;
use super::ir::*;

/// A mapping from a reference-use span (e.g. callee ident span in a call) to the
/// span of its resolved declaration.
pub type ResolveMap = FxHashMap<Span, Span>;

/// A call edge between two CFG bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallEdge {
    /// Span of the `f(..)` call expression.
    pub call_span: Span,
    /// Caller body.
    pub caller_body: BodyId,
    /// Basic block in which the call expression appears.
    pub caller_block: BlockId,
    /// Callee body.
    pub callee_body: BodyId,
}

/// Interprocedural control-flow information built on top of [`CfgCollection`].
#[derive(Debug, Clone)]
pub struct InterproceduralCfg {
    /// The underlying per-body CFGs.
    pub cfgs: CfgCollection,
    /// Call edges discovered in the syntax tree.
    pub calls: Vec<CallEdge>,
}

/// Builds a [`ResolveMap`] from an [`ExprInfo`] resolve table.
///
/// The resulting map can be passed to [`build_interprocedural_cfg`] to enable
/// call edges for `let`-bound closures and imported symbols without requiring a
/// separate resolver pass.
///
/// This is best-effort: only references that can be traced back to a concrete
/// definition span (e.g. `Decl::Func` / `Decl::Var`) are included.
pub fn resolve_map_from_expr_info(ei: &ExprInfo) -> ResolveMap {
    fn resolved_def_span(reference: &AnalysisRefExpr) -> Option<Span> {
        let mut visited: FxHashSet<crate::ty::Interned<AnalysisRefExpr>> = FxHashSet::default();
        let mut stack: Vec<Expr> = Vec::new();

        if let Some(step) = reference.step.clone() {
            stack.push(step);
        }
        if let Some(root) = reference.root.clone() {
            stack.push(root);
        }

        while let Some(expr) = stack.pop() {
            match expr {
                Expr::Decl(decl) => {
                    if decl.is_def() {
                        return Some(decl.span());
                    }
                }
                Expr::Ref(r) => {
                    if visited.insert(r.clone()) {
                        if let Some(step) = r.step.clone() {
                            stack.push(step);
                        }
                        if let Some(root) = r.root.clone() {
                            stack.push(root);
                        }
                    }
                }
                Expr::Select(select) => {
                    stack.push(select.lhs.clone());
                }
                _ => {}
            }
        }

        None
    }

    let mut out = ResolveMap::default();
    for (&use_span, reference) in ei.resolves.iter() {
        if use_span.is_detached() {
            continue;
        }
        if let Some(def_span) = resolved_def_span(reference.as_ref()) {
            out.insert(use_span, def_span);
        }
    }
    out
}

/// Builds per-body CFGs plus best-effort call edges between bodies.
///
/// `resolves` can optionally map callee identifier spans at call sites to their
/// resolved declaration spans, enabling call edges for `let`-bound closures.
pub fn build_interprocedural_cfg(
    root: &SyntaxNode,
    resolves: Option<&ResolveMap>,
) -> InterproceduralCfg {
    build_interprocedural_cfg_many(std::iter::once(root), resolves)
}

/// Builds CFGs (for multiple roots) plus best-effort call edges between bodies.
///
/// This variant enables building a project-wide interprocedural CFG by passing
/// all file roots. If `resolves` maps call-site spans to declaration spans,
/// call edges can connect across files as well.
pub fn build_interprocedural_cfg_many<'a>(
    roots: impl IntoIterator<Item = &'a SyntaxNode>,
    resolves: Option<&ResolveMap>,
) -> InterproceduralCfg {
    let roots: Vec<&SyntaxNode> = roots.into_iter().collect();
    let cfgs = build_cfgs_many(roots.iter().copied());
    if cfgs.bodies.is_empty() {
        return InterproceduralCfg {
            cfgs,
            calls: Vec::new(),
        };
    }

    let mut stmt_locs: FxHashMap<Span, (BodyId, BlockId)> = FxHashMap::default();
    for body in &cfgs.bodies {
        for (bb_idx, bb) in body.blocks.iter().enumerate() {
            let bb_id = BlockId(bb_idx);
            for stmt in &bb.stmts {
                stmt_locs.entry(stmt.span).or_insert((body.id, bb_id));
            }
        }
    }

    fn unwrap_parens<'a>(mut e: ast::Expr<'a>) -> ast::Expr<'a> {
        loop {
            match e {
                ast::Expr::Parenthesized(p) => e = p.expr(),
                _ => return e,
            }
        }
    }

    fn callee_body<'a>(
        cfgs: &CfgCollection,
        resolves: Option<&ResolveMap>,
        callee_expr: ast::Expr<'a>,
    ) -> Option<BodyId> {
        match callee_expr {
            ast::Expr::Closure(c) => cfgs.closure_body(c.span()),
            ast::Expr::Ident(ident) => resolves
                .and_then(|m| m.get(&ident.span()).copied())
                .and_then(|decl_span| cfgs.decl_body(decl_span)),
            ast::Expr::FieldAccess(access) => {
                let field = access.field();
                resolves
                    .and_then(|m| m.get(&field.span()).copied())
                    .and_then(|decl_span| cfgs.decl_body(decl_span))
            }
            _ => None,
        }
    }

    fn collect_calls<'a>(
        node: &'a SyntaxNode,
        cfgs: &CfgCollection,
        stmt_locs: &FxHashMap<Span, (BodyId, BlockId)>,
        resolves: Option<&ResolveMap>,
        out: &mut Vec<CallEdge>,
    ) {
        for child in node.children() {
            if let Some(expr) = child.cast::<ast::Expr<'a>>() {
                if let ast::Expr::FuncCall(call) = expr {
                    let call_span = call.span();
                    let callee_expr = unwrap_parens(call.callee());
                    let callee_body = callee_body(cfgs, resolves, callee_expr);

                    if let (Some(callee_body), Some((caller_body, caller_block))) =
                        (callee_body, stmt_locs.get(&call_span).copied())
                    {
                        out.push(CallEdge {
                            call_span,
                            caller_body,
                            caller_block,
                            callee_body,
                        });
                    }
                }

                collect_calls(expr.to_untyped(), cfgs, stmt_locs, resolves, out);
            } else {
                collect_calls(child, cfgs, stmt_locs, resolves, out);
            }
        }
    }

    let mut calls = Vec::new();
    for root in roots {
        collect_calls(root, &cfgs, &stmt_locs, resolves, &mut calls);
    }

    InterproceduralCfg { cfgs, calls }
}
