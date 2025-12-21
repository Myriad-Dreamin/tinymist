use typst::diag::{EcoString, SourceDiagnostic, eco_format};
use typst::syntax::ast;
use typst::syntax::ast::AstNode;

use tinymist_analysis::flow::cfg::NodeId;
use tinymist_analysis::flow::dataflow::{BackwardDataflowProblem, solve_backward};
use tinymist_analysis::flow::typst::{Node, NodeKind, StmtCfg, build_stmt_cfg_with_side_table};

use crate::DiagnosticVec;

#[derive(Debug)]
struct DiscardByReturnAnalysis {
    cfg: StmtCfg<()>,
    warn_meta_by_node: Vec<Option<WarnMeta>>,
    reachable: Vec<bool>,
    kind_at: Vec<MustReturnKind>,
    warn_here: Vec<bool>,
}

#[derive(Default)]
pub(crate) struct DiscardByReturnCache {
    by_span: std::collections::HashMap<u64, std::sync::Arc<DiscardByReturnAnalysis>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MustReturnKind {
    Unreachable,
    No,
    Value,
    None,
}

impl MustReturnKind {
    fn join(self, other: Self) -> Self {
        match (self, other) {
            (MustReturnKind::Unreachable, x) | (x, MustReturnKind::Unreachable) => x,
            (MustReturnKind::Value, MustReturnKind::Value) => MustReturnKind::Value,
            (MustReturnKind::None, MustReturnKind::None) => MustReturnKind::None,
            _ => MustReturnKind::No,
        }
    }
}

#[derive(Debug, Clone)]
struct WarnMeta {
    kind_name: EcoString,
    is_show_or_set: bool,
    is_hashable: bool,
    expr_text: Option<EcoString>,
}

impl WarnMeta {
    fn warnable_for(&self, kind: MustReturnKind) -> bool {
        match kind {
            MustReturnKind::Unreachable => false,
            MustReturnKind::No => false,
            MustReturnKind::Value => true,
            MustReturnKind::None => self.is_show_or_set,
        }
    }
}

struct MustReturnKindProblem;

impl BackwardDataflowProblem<Node<()>> for MustReturnKindProblem {
    type State = MustReturnKind;

    fn bottom(&self) -> Self::State {
        MustReturnKind::Unreachable
    }

    fn join(&self, left: &Self::State, right: &Self::State) -> Self::State {
        (*left).join(*right)
    }

    fn transfer(&self, _node_id: NodeId, node: &Node<()>, out_state: &Self::State) -> Self::State {
        match node.kind {
            NodeKind::Exit => MustReturnKind::No,
            NodeKind::Return { has_value } => {
                if has_value {
                    MustReturnKind::Value
                } else {
                    MustReturnKind::None
                }
            }
            _ => *out_state,
        }
    }
}

fn warn_meta(expr: ast::Expr<'_>) -> Option<WarnMeta> {
    if expr.to_untyped().kind().is_trivia() {
        return None;
    }
    if matches!(expr, ast::Expr::None(..)) {
        return None;
    }

    // Match the old `LateFuncLinter::value` coverage: only warn for expressions
    // that commonly look like "content statements" rather than "effectful
    // statements".
    let is_show_or_set = matches!(expr, ast::Expr::ShowRule(..) | ast::Expr::SetRule(..));
    let warnable = matches!(
        expr,
        ast::Expr::Text(..)
            | ast::Expr::Space(..)
            | ast::Expr::Linebreak(..)
            | ast::Expr::Parbreak(..)
            | ast::Expr::Escape(..)
            | ast::Expr::Shorthand(..)
            | ast::Expr::SmartQuote(..)
            | ast::Expr::Raw(..)
            | ast::Expr::Link(..)
            | ast::Expr::Label(..)
            | ast::Expr::Ref(..)
            | ast::Expr::Auto(..)
            | ast::Expr::Bool(..)
            | ast::Expr::Int(..)
            | ast::Expr::Float(..)
            | ast::Expr::Numeric(..)
            | ast::Expr::Str(..)
            | ast::Expr::MathText(..)
            | ast::Expr::MathShorthand(..)
            | ast::Expr::MathAlignPoint(..)
            | ast::Expr::MathPrimes(..)
            | ast::Expr::MathRoot(..)
            | ast::Expr::Equation(..)
            | ast::Expr::Array(..)
            | ast::Expr::Dict(..)
            | ast::Expr::ModuleInclude(..)
            | ast::Expr::ShowRule(..)
            | ast::Expr::SetRule(..)
    );

    if !warnable {
        return None;
    }

    let kind_name: EcoString = expr.to_untyped().kind().name().into();

    let is_hashable = expr.hash();
    let expr_text = (is_hashable && !is_show_or_set).then(|| expr.to_untyped().clone().into_text());

    Some(WarnMeta {
        kind_name,
        is_show_or_set,
        is_hashable,
        expr_text,
    })
}

struct WarnCoverageProblem<'a> {
    reachable: &'a [bool],
    kind_at: &'a [MustReturnKind],
    warn_meta_by_node: &'a [Option<WarnMeta>],
}

impl BackwardDataflowProblem<Node<()>> for WarnCoverageProblem<'_> {
    /// Whether this node is already "covered" by a warning on all paths to the
    /// relevant `return`.
    ///
    /// This is a diagnostic policy state (not a program semantic fact) and is
    /// kept separate from the semantic analysis lattice (`MustReturnKind`).
    type State = bool;

    fn bottom(&self) -> Self::State {
        true
    }

    fn join(&self, left: &Self::State, right: &Self::State) -> Self::State {
        *left && *right
    }

    fn transfer(
        &self,
        node_id: NodeId,
        node: &Node<()>,
        succ_covered: &Self::State,
    ) -> Self::State {
        if !self.reachable[node_id.index()] {
            return true;
        }
        if matches!(node.kind, NodeKind::Return { .. }) {
            return false;
        }

        let kind = self.kind_at[node_id.index()];
        if matches!(kind, MustReturnKind::Unreachable | MustReturnKind::No) {
            return true;
        }

        let warnable = matches!(node.kind, NodeKind::Stmt)
            && self.warn_meta_by_node[node_id.index()]
                .as_ref()
                .is_some_and(|m| m.warnable_for(kind));
        *succ_covered || warnable
    }
}

fn analyze_discarded_by_function_return(body: ast::Expr<'_>) -> Option<DiscardByReturnAnalysis> {
    let (cfg, warn_meta_by_node) =
        build_stmt_cfg_with_side_table(body, |expr| ((), warn_meta(expr)));
    if !cfg.has_return {
        return None;
    }

    let reachable = cfg.cfg.reachable_from_entry();
    let solution = solve_backward(&cfg.cfg, &MustReturnKindProblem);
    let kind_at = solution.in_states;
    let coverage = solve_backward(
        &cfg.cfg,
        &WarnCoverageProblem {
            reachable: &reachable,
            kind_at: &kind_at,
            warn_meta_by_node: &warn_meta_by_node,
        },
    );

    let mut warn_here = vec![false; cfg.cfg.len()];
    for (idx, node) in cfg.cfg.nodes().iter().enumerate() {
        if !reachable[idx] {
            continue;
        }
        if matches!(node.kind, NodeKind::Return { .. }) {
            continue;
        }
        if !matches!(node.kind, NodeKind::Stmt) {
            continue;
        }

        let kind = kind_at[idx];
        if matches!(kind, MustReturnKind::Unreachable | MustReturnKind::No) {
            continue;
        }

        let Some(meta) = warn_meta_by_node[idx].as_ref() else {
            continue;
        };
        if !meta.warnable_for(kind) {
            continue;
        }

        let succ_covered = coverage.out_states[idx];
        warn_here[idx] = !succ_covered;
    }

    Some(DiscardByReturnAnalysis {
        cfg,
        warn_meta_by_node,
        reachable,
        kind_at,
        warn_here,
    })
}

fn emit_discarded_by_function_return(diag: &mut DiagnosticVec, analysis: &DiscardByReturnAnalysis) {
    let cfg = &analysis.cfg.cfg;

    for (id, node) in cfg.nodes().iter().enumerate() {
        if !analysis.reachable[id] || !analysis.warn_here[id] {
            continue;
        }

        let Some(meta) = analysis.warn_meta_by_node[id].as_ref() else {
            continue;
        };

        let kind = analysis.kind_at[id];
        if !meta.warnable_for(kind) {
            continue;
        }

        let mut diag_ = SourceDiagnostic::warning(
            node.span,
            eco_format!(
                "This {} is implicitly discarded by function return",
                meta.kind_name
            ),
        );

        if kind == MustReturnKind::Value
            && !meta.is_show_or_set
            && meta.is_hashable
            && let Some(text) = meta.expr_text.as_ref()
        {
            diag_.hint(eco_format!(
                "consider ignoring the value explicitly using underscore: `let _ = {}`",
                text
            ));
        }

        diag.push(diag_);
    }
}

pub(crate) fn lint_discarded_by_function_return_cached(
    cache: &mut DiscardByReturnCache,
    diag: &mut DiagnosticVec,
    body: ast::Expr<'_>,
) {
    let key = body.span().into_raw().get();
    if let Some(analysis) = cache.by_span.get(&key) {
        emit_discarded_by_function_return(diag, analysis);
        return;
    }

    let Some(analysis) = analyze_discarded_by_function_return(body) else {
        return;
    };
    let analysis = std::sync::Arc::new(analysis);
    emit_discarded_by_function_return(diag, &analysis);
    cache.by_span.insert(key, analysis);
}
