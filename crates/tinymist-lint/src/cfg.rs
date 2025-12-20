use std::collections::VecDeque;

use typst::diag::{EcoString, SourceDiagnostic, eco_format};
use typst::syntax::ast::AstNode;
use typst::syntax::{Span, ast};

use crate::DiagnosticVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MustReturnKind {
    No,
    Value,
    None,
}

impl MustReturnKind {
    fn join(self, other: Self) -> Self {
        match (self, other) {
            (MustReturnKind::Value, MustReturnKind::Value) => MustReturnKind::Value,
            (MustReturnKind::None, MustReturnKind::None) => MustReturnKind::None,
            _ => MustReturnKind::No,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MustReturnState {
    kind: MustReturnKind,
    /// Whether a warning has already been emitted on all paths to the return.
    ///
    /// This matches the old `ReturnBlockInfo.warned` behavior (AND-merge across
    /// branches) to keep diagnostics from becoming too noisy.
    warned: bool,
}

impl MustReturnState {
    const NO: Self = Self {
        kind: MustReturnKind::No,
        warned: true,
    };

    fn join(self, other: Self) -> Self {
        let kind = self.kind.join(other.kind);
        if kind == MustReturnKind::No {
            return Self::NO;
        }
        Self {
            kind,
            warned: self.warned && other.warned,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeKind {
    Entry,
    Exit,
    Branch,
    LoopHead,
    Join,
    Break,
    Continue,
    Return { has_value: bool },
    Stmt,
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
            MustReturnKind::No => false,
            MustReturnKind::Value => true,
            MustReturnKind::None => self.is_show_or_set,
        }
    }
}

#[derive(Debug, Clone)]
struct Node {
    kind: NodeKind,
    span: Span,
    warn: Option<WarnMeta>,
}

#[derive(Debug, Default)]
struct Cfg {
    nodes: Vec<Node>,
    succ: Vec<Vec<usize>>,
    entry: usize,
    exit: usize,
    has_return: bool,
}

impl Cfg {
    fn add_node(&mut self, node: Node) -> usize {
        let id = self.nodes.len();
        self.nodes.push(node);
        self.succ.push(Vec::new());
        id
    }

    fn add_edge(&mut self, from: usize, to: usize) {
        self.succ[from].push(to);
    }

    fn reachable_from_entry(&self) -> Vec<bool> {
        let mut reachable = vec![false; self.nodes.len()];
        let mut q = VecDeque::new();
        reachable[self.entry] = true;
        q.push_back(self.entry);

        while let Some(n) = q.pop_front() {
            for &m in &self.succ[n] {
                if !reachable[m] {
                    reachable[m] = true;
                    q.push_back(m);
                }
            }
        }

        reachable
    }

    fn compute_states(&self) -> Vec<MustReturnState> {
        let mut states = vec![MustReturnState::NO; self.nodes.len()];

        for (id, node) in self.nodes.iter().enumerate() {
            match node.kind {
                NodeKind::Exit => states[id] = MustReturnState::NO,
                NodeKind::Return { has_value } => {
                    states[id] = MustReturnState {
                        kind: if has_value {
                            MustReturnKind::Value
                        } else {
                            MustReturnKind::None
                        },
                        warned: false,
                    };
                }
                _ => {}
            }
        }

        // Small lattice (3 kinds + warned boolean), so a simple fixed-point loop
        // is sufficient.
        for _ in 0..(self.nodes.len().saturating_mul(8).max(32)) {
            let mut changed = false;

            for id in (0..self.nodes.len()).rev() {
                let node = &self.nodes[id];
                let new_state = match node.kind {
                    NodeKind::Exit => MustReturnState::NO,
                    NodeKind::Return { .. } => states[id],
                    _ => {
                        let succ_state = self.succ[id]
                            .iter()
                            .copied()
                            .map(|sid| states[sid])
                            .reduce(MustReturnState::join)
                            .unwrap_or(MustReturnState::NO);

                        if succ_state.kind == MustReturnKind::No {
                            MustReturnState::NO
                        } else {
                            let warnable = node.warn.as_ref().is_some_and(|m| {
                                m.warnable_for(succ_state.kind) && !succ_state.warned
                            });

                            MustReturnState {
                                kind: succ_state.kind,
                                warned: succ_state.warned || warnable,
                            }
                        }
                    }
                };

                if new_state != states[id] {
                    states[id] = new_state;
                    changed = true;
                }
            }

            if !changed {
                break;
            }
        }

        states
    }
}

#[derive(Debug, Clone, Copy)]
struct LoopCtx {
    head: usize,
    after: usize,
}

struct Builder {
    cfg: Cfg,
    loops: Vec<LoopCtx>,
}

impl Builder {
    fn new(span: Span) -> Self {
        let mut cfg = Cfg::default();

        let exit = cfg.add_node(Node {
            kind: NodeKind::Exit,
            span,
            warn: None,
        });
        let entry = cfg.add_node(Node {
            kind: NodeKind::Entry,
            span,
            warn: None,
        });

        cfg.exit = exit;
        cfg.entry = entry;

        Self {
            cfg,
            loops: Vec::new(),
        }
    }

    fn build_body(mut self, body: ast::Expr<'_>) -> Cfg {
        let exits = self.build_stmt(body, vec![self.cfg.entry]);
        for e in exits {
            self.cfg.add_edge(e, self.cfg.exit);
        }
        self.cfg
    }

    fn connect_open(&mut self, open: &[usize], to: usize) {
        for &p in open {
            self.cfg.add_edge(p, to);
        }
    }

    fn build_seq<'a>(
        &mut self,
        exprs: impl IntoIterator<Item = ast::Expr<'a>>,
        mut open: Vec<usize>,
    ) -> Vec<usize> {
        for expr in exprs {
            open = self.build_stmt(expr, open);
        }
        open
    }

    fn build_stmt<'a>(&mut self, expr: ast::Expr<'a>, open: Vec<usize>) -> Vec<usize> {
        match expr {
            ast::Expr::Parenthesized(p) => self.build_stmt(p.expr(), open),
            ast::Expr::CodeBlock(b) => self.build_seq(b.body().exprs(), open),
            ast::Expr::ContentBlock(b) => self.build_seq(b.body().exprs(), open),

            ast::Expr::Conditional(c) => self.build_conditional(c, open),
            ast::Expr::WhileLoop(l) => self.build_loop(l.body(), open),
            ast::Expr::ForLoop(l) => self.build_loop(l.body(), open),

            // Treat nested functions as atomic: their `return` does not affect
            // the enclosing function.
            ast::Expr::Closure(..) | ast::Expr::Contextual(..) => self.build_simple(expr, open),

            ast::Expr::LoopBreak(b) => self.build_break(b, open),
            ast::Expr::LoopContinue(c) => self.build_continue(c, open),
            ast::Expr::FuncReturn(r) => self.build_return(r, open),

            _ => self.build_simple(expr, open),
        }
    }

    fn build_simple<'a>(&mut self, expr: ast::Expr<'a>, open: Vec<usize>) -> Vec<usize> {
        let span = expr.span();
        let warn = warn_meta(expr);
        let id = self.cfg.add_node(Node {
            kind: NodeKind::Stmt,
            span,
            warn,
        });
        self.connect_open(&open, id);
        vec![id]
    }

    fn build_conditional<'a>(
        &mut self,
        expr: ast::Conditional<'a>,
        open: Vec<usize>,
    ) -> Vec<usize> {
        let branch = self.cfg.add_node(Node {
            kind: NodeKind::Branch,
            span: expr.span(),
            warn: None,
        });
        let after = self.cfg.add_node(Node {
            kind: NodeKind::Join,
            span: expr.span(),
            warn: None,
        });

        self.connect_open(&open, branch);

        // If branch.
        let if_exits = self.build_stmt(expr.if_body(), vec![branch]);
        for e in if_exits {
            self.cfg.add_edge(e, after);
        }

        // Else branch.
        if let Some(else_body) = expr.else_body() {
            let else_exits = self.build_stmt(else_body, vec![branch]);
            for e in else_exits {
                self.cfg.add_edge(e, after);
            }
        } else {
            self.cfg.add_edge(branch, after);
        }

        vec![after]
    }

    fn build_loop<'a>(&mut self, body: ast::Expr<'a>, open: Vec<usize>) -> Vec<usize> {
        let span = body.span();
        let head = self.cfg.add_node(Node {
            kind: NodeKind::LoopHead,
            span,
            warn: None,
        });
        let after = self.cfg.add_node(Node {
            kind: NodeKind::Join,
            span,
            warn: None,
        });
        self.connect_open(&open, head);

        self.cfg.add_edge(head, after);

        self.loops.push(LoopCtx { head, after });
        let body_exits = self.build_stmt(body, vec![head]);
        self.loops.pop();

        for e in body_exits {
            self.cfg.add_edge(e, head);
        }

        vec![after]
    }

    fn build_break(&mut self, expr: ast::LoopBreak<'_>, open: Vec<usize>) -> Vec<usize> {
        let id = self.cfg.add_node(Node {
            kind: NodeKind::Break,
            span: expr.span(),
            warn: None,
        });
        self.connect_open(&open, id);

        if let Some(loop_ctx) = self.loops.last().copied() {
            self.cfg.add_edge(id, loop_ctx.after);
            Vec::new()
        } else {
            // Invalid `break`: terminate the path conservatively so we don't
            // emit follow-up diagnostics that depend on well-formed control
            // flow.
            self.cfg.add_edge(id, self.cfg.exit);
            Vec::new()
        }
    }

    fn build_continue(&mut self, expr: ast::LoopContinue<'_>, open: Vec<usize>) -> Vec<usize> {
        let id = self.cfg.add_node(Node {
            kind: NodeKind::Continue,
            span: expr.span(),
            warn: None,
        });
        self.connect_open(&open, id);

        if let Some(loop_ctx) = self.loops.last().copied() {
            self.cfg.add_edge(id, loop_ctx.head);
            Vec::new()
        } else {
            self.cfg.add_edge(id, self.cfg.exit);
            Vec::new()
        }
    }

    fn build_return(&mut self, expr: ast::FuncReturn<'_>, open: Vec<usize>) -> Vec<usize> {
        let has_value = expr.body().is_some();
        let id = self.cfg.add_node(Node {
            kind: NodeKind::Return { has_value },
            span: expr.span(),
            warn: None,
        });
        self.cfg.has_return = true;
        self.connect_open(&open, id);
        self.cfg.add_edge(id, self.cfg.exit);
        Vec::new()
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

pub(crate) fn lint_discarded_by_function_return(diag: &mut DiagnosticVec, body: ast::Expr<'_>) {
    let cfg = Builder::new(body.span()).build_body(body);
    if !cfg.has_return {
        return;
    }

    let reachable = cfg.reachable_from_entry();
    let states = cfg.compute_states();

    for (id, node) in cfg.nodes.iter().enumerate() {
        if !reachable[id] {
            continue;
        }

        let Some(meta) = node.warn.as_ref() else {
            continue;
        };

        let succ_state = cfg.succ[id]
            .iter()
            .copied()
            .map(|sid| states[sid])
            .reduce(MustReturnState::join)
            .unwrap_or(MustReturnState::NO);

        if succ_state.kind == MustReturnKind::No || succ_state.warned {
            continue;
        }
        if !meta.warnable_for(succ_state.kind) {
            continue;
        }

        let mut diag_ = SourceDiagnostic::warning(
            node.span,
            eco_format!(
                "This {} is implicitly discarded by function return",
                meta.kind_name
            ),
        );

        if succ_state.kind == MustReturnKind::Value
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
