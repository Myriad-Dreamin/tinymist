//! Typst AST lowering into a statement-level CFG.
use super::cfg::{Cfg, NodeId};
use typst::syntax::ast::AstNode;
use typst::syntax::{Span, ast};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Node kind for the statement-level CFG.
pub enum NodeKind {
    /// CFG entry.
    Entry,
    /// CFG exit.
    Exit,
    /// Loop header (back-edge target).
    LoopHead,
    /// Join point for branches / loop exits.
    Join,
    /// `break` terminator.
    Break,
    /// `continue` terminator.
    Continue,
    /// `return` terminator.
    Return {
        /// Whether the `return` has an explicit value.
        has_value: bool,
    },
    /// A regular statement/expression node.
    Stmt,
    /// The condition expression of an `if`.
    IfCond,
    /// The condition expression of a `while`.
    WhileCond,
    /// The iterable expression of a `for`.
    ForIterable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Edge label for a statement-level Typst CFG.
pub enum StmtEdge {
    /// Regular, unconditional control-flow.
    Normal,
    /// Control-flow taken when `cond` evaluates to `true`.
    CondTrue {
        /// The condition expression node that guards this edge.
        cond: NodeId,
    },
    /// Control-flow taken when `cond` evaluates to `false`.
    CondFalse {
        /// The condition expression node that guards this edge.
        cond: NodeId,
    },
}

impl Default for StmtEdge {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Copy)]
struct OpenEdge {
    from: NodeId,
    edge: StmtEdge,
}

impl OpenEdge {
    fn normal(from: NodeId) -> Self {
        Self {
            from,
            edge: StmtEdge::Normal,
        }
    }

    fn cond_true(from: NodeId) -> Self {
        Self {
            from,
            edge: StmtEdge::CondTrue { cond: from },
        }
    }

    fn cond_false(from: NodeId) -> Self {
        Self {
            from,
            edge: StmtEdge::CondFalse { cond: from },
        }
    }
}

#[derive(Debug, Clone)]
/// A CFG node with span and analysis payload.
pub struct Node<D> {
    /// Control-flow category.
    pub kind: NodeKind,
    /// Source span for diagnostics and mapping.
    pub span: Span,
    /// Analysis payload stored on the node.
    pub data: D,
}

#[derive(Debug, Clone)]
/// A statement-level CFG built from a Typst expression body.
pub struct StmtCfg<D> {
    /// The underlying control-flow graph.
    pub cfg: Cfg<Node<D>, StmtEdge>,
    /// Whether the body contains a `return`.
    pub has_return: bool,
}

/// Builds a statement-level CFG and a per-node side table in a single pass.
///
/// The returned `Vec<S>` is indexed by `NodeId` (`side[node_id.index()]`).
pub fn build_stmt_cfg_with_side_table<D: Default, S: Default>(
    body: ast::Expr<'_>,
    mut stmt_payload: impl FnMut(ast::Expr<'_>) -> (D, S),
) -> (StmtCfg<D>, Vec<S>) {
    let mut side = vec![S::default(), S::default()];
    let cfg =
        Builder::<D>::new(body.span()).build_body_with_side(body, &mut stmt_payload, &mut side);
    debug_assert_eq!(cfg.cfg.len(), side.len());
    (cfg, side)
}

/// Builds a statement-level CFG for the given Typst expression body.
pub fn build_stmt_cfg<D: Default>(
    body: ast::Expr<'_>,
    stmt_data: impl FnMut(ast::Expr<'_>) -> D,
) -> StmtCfg<D> {
    let mut stmt_data = stmt_data;
    let (cfg, _) = build_stmt_cfg_with_side_table(body, |expr| (stmt_data(expr), ()));
    cfg
}

#[derive(Debug, Clone, Copy)]
struct LoopCtx {
    head: NodeId,
    after: NodeId,
}

struct Builder<D> {
    cfg: Cfg<Node<D>, StmtEdge>,
    loops: Vec<LoopCtx>,
    has_return: bool,
}

impl<D: Default> Builder<D> {
    fn new(span: Span) -> Self {
        let entry = Node {
            kind: NodeKind::Entry,
            span,
            data: D::default(),
        };
        let exit = Node {
            kind: NodeKind::Exit,
            span,
            data: D::default(),
        };

        Self {
            cfg: Cfg::new(entry, exit),
            loops: Vec::new(),
            has_return: false,
        }
    }

    fn build_body_with_side<S: Default>(
        mut self,
        body: ast::Expr<'_>,
        stmt_payload: &mut impl FnMut(ast::Expr<'_>) -> (D, S),
        side: &mut Vec<S>,
    ) -> StmtCfg<D> {
        let exits = self.build_stmt(
            body,
            vec![OpenEdge::normal(self.cfg.entry)],
            stmt_payload,
            side,
        );
        for e in exits {
            self.cfg.add_edge_with(e.from, self.cfg.exit, e.edge);
        }
        debug_assert_stmt_cfg_well_formed(&self.cfg);
        StmtCfg {
            cfg: self.cfg,
            has_return: self.has_return,
        }
    }

    fn connect_open(&mut self, open: &[OpenEdge], to: NodeId) {
        for e in open {
            self.cfg.add_edge_with(e.from, to, e.edge);
        }
    }

    fn build_seq<'a, S: Default>(
        &mut self,
        exprs: impl IntoIterator<Item = ast::Expr<'a>>,
        mut open: Vec<OpenEdge>,
        stmt_payload: &mut impl FnMut(ast::Expr<'a>) -> (D, S),
        side: &mut Vec<S>,
    ) -> Vec<OpenEdge> {
        for expr in exprs {
            open = self.build_stmt(expr, open, stmt_payload, side);
        }
        open
    }

    fn build_stmt<'a, S: Default>(
        &mut self,
        expr: ast::Expr<'a>,
        open: Vec<OpenEdge>,
        stmt_payload: &mut impl FnMut(ast::Expr<'a>) -> (D, S),
        side: &mut Vec<S>,
    ) -> Vec<OpenEdge> {
        match expr {
            ast::Expr::Parenthesized(p) => self.build_stmt(p.expr(), open, stmt_payload, side),
            ast::Expr::CodeBlock(b) => self.build_seq(b.body().exprs(), open, stmt_payload, side),
            ast::Expr::ContentBlock(b) => {
                self.build_seq(b.body().exprs(), open, stmt_payload, side)
            }

            ast::Expr::Conditional(c) => self.build_conditional(c, open, stmt_payload, side),
            ast::Expr::WhileLoop(l) => self.build_while_loop(l, open, stmt_payload, side),
            ast::Expr::ForLoop(l) => self.build_for_loop(l, open, stmt_payload, side),

            // Treat nested functions as atomic: their `return` does not affect
            // the enclosing function.
            ast::Expr::Closure(..) | ast::Expr::Contextual(..) => {
                self.build_simple(expr, open, stmt_payload, side)
            }

            ast::Expr::LoopBreak(b) => self.build_break(b, open, side),
            ast::Expr::LoopContinue(c) => self.build_continue(c, open, side),
            ast::Expr::FuncReturn(r) => self.build_return(r, open, side),

            _ => self.build_simple(expr, open, stmt_payload, side),
        }
    }

    fn build_expr_node<'a, S: Default>(
        &mut self,
        kind: NodeKind,
        expr: ast::Expr<'a>,
        open: &[OpenEdge],
        stmt_payload: &mut impl FnMut(ast::Expr<'a>) -> (D, S),
        side: &mut Vec<S>,
    ) -> NodeId {
        let (data, side_data) = stmt_payload(expr);
        let id = self.cfg.add_node(Node {
            kind,
            span: expr.span(),
            data,
        });
        side.push(side_data);
        debug_assert_eq!(id.index(), side.len() - 1);
        self.connect_open(open, id);
        id
    }

    fn build_simple<'a, S: Default>(
        &mut self,
        expr: ast::Expr<'a>,
        open: Vec<OpenEdge>,
        stmt_payload: &mut impl FnMut(ast::Expr<'a>) -> (D, S),
        side: &mut Vec<S>,
    ) -> Vec<OpenEdge> {
        let (data, side_data) = stmt_payload(expr);
        let id = self.cfg.add_node(Node {
            kind: NodeKind::Stmt,
            span: expr.span(),
            data,
        });
        side.push(side_data);
        debug_assert_eq!(id.index(), side.len() - 1);
        self.connect_open(&open, id);
        vec![OpenEdge::normal(id)]
    }

    fn build_conditional<'a, S: Default>(
        &mut self,
        expr: ast::Conditional<'a>,
        open: Vec<OpenEdge>,
        stmt_payload: &mut impl FnMut(ast::Expr<'a>) -> (D, S),
        side: &mut Vec<S>,
    ) -> Vec<OpenEdge> {
        let after = self.cfg.add_node(Node {
            kind: NodeKind::Join,
            span: expr.span(),
            data: D::default(),
        });
        side.push(S::default());
        debug_assert_eq!(after.index(), side.len() - 1);

        let (true_exits, false_exits) = self.build_bool_condition(
            NodeKind::IfCond,
            expr.condition(),
            &open,
            stmt_payload,
            side,
        );

        // If branch.
        let if_exits = self.build_stmt(
            expr.if_body(),
            true_exits.into_iter().map(OpenEdge::cond_true).collect(),
            stmt_payload,
            side,
        );
        for e in if_exits {
            self.cfg.add_edge_with(e.from, after, e.edge);
        }

        // Else branch.
        if let Some(else_body) = expr.else_body() {
            let else_exits = self.build_stmt(
                else_body,
                false_exits.into_iter().map(OpenEdge::cond_false).collect(),
                stmt_payload,
                side,
            );
            for e in else_exits {
                self.cfg.add_edge_with(e.from, after, e.edge);
            }
        } else {
            self.connect_open(
                &false_exits.into_iter().map(OpenEdge::cond_false).collect::<Vec<_>>(),
                after,
            );
        }

        vec![OpenEdge::normal(after)]
    }

    fn build_while_loop<'a, S: Default>(
        &mut self,
        expr: ast::WhileLoop<'a>,
        open: Vec<OpenEdge>,
        stmt_payload: &mut impl FnMut(ast::Expr<'a>) -> (D, S),
        side: &mut Vec<S>,
    ) -> Vec<OpenEdge> {
        let span = expr.span();
        let head = self.cfg.add_node(Node {
            kind: NodeKind::LoopHead,
            span,
            data: D::default(),
        });
        side.push(S::default());
        debug_assert_eq!(head.index(), side.len() - 1);

        let after = self.cfg.add_node(Node {
            kind: NodeKind::Join,
            span,
            data: D::default(),
        });
        side.push(S::default());
        debug_assert_eq!(after.index(), side.len() - 1);

        self.connect_open(&open, head);

        let (true_exits, false_exits) = self.build_bool_condition(
            NodeKind::WhileCond,
            expr.condition(),
            &[OpenEdge::normal(head)],
            stmt_payload,
            side,
        );
        self.connect_open(
            &false_exits.into_iter().map(OpenEdge::cond_false).collect::<Vec<_>>(),
            after,
        );

        self.loops.push(LoopCtx { head, after });
        let body_exits = self.build_stmt(
            expr.body(),
            true_exits.into_iter().map(OpenEdge::cond_true).collect(),
            stmt_payload,
            side,
        );
        self.loops.pop();

        for e in body_exits {
            self.cfg.add_edge_with(e.from, head, e.edge);
        }

        vec![OpenEdge::normal(after)]
    }

    fn build_for_loop<'a, S: Default>(
        &mut self,
        expr: ast::ForLoop<'a>,
        open: Vec<OpenEdge>,
        stmt_payload: &mut impl FnMut(ast::Expr<'a>) -> (D, S),
        side: &mut Vec<S>,
    ) -> Vec<OpenEdge> {
        let span = expr.span();
        // Model the iterable expression as an explicit node executed once.
        let iterable = self.build_expr_node(
            NodeKind::ForIterable,
            expr.iterable(),
            &open,
            stmt_payload,
            side,
        );

        let head = self.cfg.add_node(Node {
            kind: NodeKind::LoopHead,
            span,
            data: D::default(),
        });
        side.push(S::default());
        debug_assert_eq!(head.index(), side.len() - 1);

        let after = self.cfg.add_node(Node {
            kind: NodeKind::Join,
            span,
            data: D::default(),
        });
        side.push(S::default());
        debug_assert_eq!(after.index(), side.len() - 1);

        self.cfg.add_edge_with(iterable, head, StmtEdge::Normal);
        self.cfg.add_edge_with(head, after, StmtEdge::Normal);

        self.loops.push(LoopCtx { head, after });
        let body_exits = self.build_stmt(
            expr.body(),
            vec![OpenEdge::normal(head)],
            stmt_payload,
            side,
        );
        self.loops.pop();

        for e in body_exits {
            self.cfg.add_edge_with(e.from, head, e.edge);
        }

        vec![OpenEdge::normal(after)]
    }

    fn build_break<S: Default>(
        &mut self,
        expr: ast::LoopBreak<'_>,
        open: Vec<OpenEdge>,
        side: &mut Vec<S>,
    ) -> Vec<OpenEdge> {
        let id = self.cfg.add_node(Node {
            kind: NodeKind::Break,
            span: expr.span(),
            data: D::default(),
        });
        side.push(S::default());
        debug_assert_eq!(id.index(), side.len() - 1);
        self.connect_open(&open, id);

        if let Some(loop_ctx) = self.loops.last().copied() {
            self.cfg.add_edge_with(id, loop_ctx.after, StmtEdge::Normal);
        } else {
            self.cfg.add_edge_with(id, self.cfg.exit, StmtEdge::Normal);
        }
        Vec::new()
    }

    fn build_continue<S: Default>(
        &mut self,
        expr: ast::LoopContinue<'_>,
        open: Vec<OpenEdge>,
        side: &mut Vec<S>,
    ) -> Vec<OpenEdge> {
        let id = self.cfg.add_node(Node {
            kind: NodeKind::Continue,
            span: expr.span(),
            data: D::default(),
        });
        side.push(S::default());
        debug_assert_eq!(id.index(), side.len() - 1);
        self.connect_open(&open, id);

        if let Some(loop_ctx) = self.loops.last().copied() {
            self.cfg.add_edge_with(id, loop_ctx.head, StmtEdge::Normal);
        } else {
            self.cfg.add_edge_with(id, self.cfg.exit, StmtEdge::Normal);
        }
        Vec::new()
    }

    fn build_return<S: Default>(
        &mut self,
        expr: ast::FuncReturn<'_>,
        open: Vec<OpenEdge>,
        side: &mut Vec<S>,
    ) -> Vec<OpenEdge> {
        let has_value = expr.body().is_some();
        let id = self.cfg.add_node(Node {
            kind: NodeKind::Return { has_value },
            span: expr.span(),
            data: D::default(),
        });
        side.push(S::default());
        debug_assert_eq!(id.index(), side.len() - 1);
        self.has_return = true;
        self.connect_open(&open, id);
        self.cfg.add_edge_with(id, self.cfg.exit, StmtEdge::Normal);
        Vec::new()
    }

    fn build_bool_condition<'a, S: Default>(
        &mut self,
        kind: NodeKind,
        expr: ast::Expr<'a>,
        open: &[OpenEdge],
        stmt_payload: &mut impl FnMut(ast::Expr<'a>) -> (D, S),
        side: &mut Vec<S>,
    ) -> (Vec<NodeId>, Vec<NodeId>) {
        match expr {
            ast::Expr::Parenthesized(p) => {
                self.build_bool_condition(kind, p.expr(), open, stmt_payload, side)
            }
            ast::Expr::Unary(u) if matches!(u.op(), ast::UnOp::Not) => {
                let (t, f) = self.build_bool_condition(kind, u.expr(), open, stmt_payload, side);
                (f, t)
            }
            ast::Expr::Binary(b) if matches!(b.op(), ast::BinOp::And | ast::BinOp::Or) => {
                match b.op() {
                    ast::BinOp::And => {
                        let (lhs_t, mut lhs_f) =
                            self.build_bool_condition(kind, b.lhs(), open, stmt_payload, side);
                        let rhs_open: Vec<OpenEdge> =
                            lhs_t.iter().copied().map(OpenEdge::cond_true).collect();
                        let (rhs_t, mut rhs_f) =
                            self.build_bool_condition(kind, b.rhs(), &rhs_open, stmt_payload, side);
                        lhs_f.append(&mut rhs_f);
                        dedup_node_ids(&mut lhs_f);
                        (rhs_t, lhs_f)
                    }
                    ast::BinOp::Or => {
                        let (mut lhs_t, lhs_f) =
                            self.build_bool_condition(kind, b.lhs(), open, stmt_payload, side);
                        let rhs_open: Vec<OpenEdge> =
                            lhs_f.iter().copied().map(OpenEdge::cond_false).collect();
                        let (mut rhs_t, rhs_f) =
                            self.build_bool_condition(kind, b.rhs(), &rhs_open, stmt_payload, side);
                        lhs_t.append(&mut rhs_t);
                        dedup_node_ids(&mut lhs_t);
                        (lhs_t, rhs_f)
                    }
                    _ => unreachable!(),
                }
            }
            _ => {
                let id = self.build_expr_node(kind, expr, open, stmt_payload, side);
                (vec![id], vec![id])
            }
        }
    }
}

fn dedup_node_ids(nodes: &mut Vec<NodeId>) {
    nodes.sort_unstable_by_key(|n| n.index());
    nodes.dedup_by_key(|n| n.index());
}

#[cfg(debug_assertions)]
fn debug_assert_stmt_cfg_well_formed<D>(cfg: &Cfg<Node<D>, StmtEdge>) {
    for idx in 0..cfg.len() {
        let id = NodeId::from(idx);
        if id == cfg.exit {
            continue;
        }
        debug_assert!(
            !cfg.successors(id).is_empty(),
            "stmt-cfg node has no successors (missing fallthrough edge?): idx={idx} kind={:?}",
            cfg.node(id).kind
        );
    }

    let from_entry = cfg.reachable_from_entry();
    let mut to_exit = vec![false; cfg.len()];
    let mut q = std::collections::VecDeque::new();
    to_exit[cfg.exit.index()] = true;
    q.push_back(cfg.exit);

    while let Some(n) = q.pop_front() {
        for &p in cfg.predecessors(n) {
            if !to_exit[p.index()] {
                to_exit[p.index()] = true;
                q.push_back(p);
            }
        }
    }

    for (idx, &reachable) in from_entry.iter().enumerate() {
        if reachable {
            debug_assert!(
                to_exit[idx],
                "stmt-cfg reachable node cannot reach exit: idx={idx} kind={:?}",
                cfg.nodes()[idx].kind
            );
        }
    }
}

#[cfg(not(debug_assertions))]
fn debug_assert_stmt_cfg_well_formed<D>(_cfg: &Cfg<Node<D>, StmtEdge>) {}
