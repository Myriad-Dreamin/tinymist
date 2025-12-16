//! Control-flow graph (CFG) construction and analysis for Typst syntax.
//!
//! This module builds CFGs directly from Typst's parsed AST (`typst::syntax::ast`),
//! so it can be used by both IDE features and linters/debug tooling.

use rustc_hash::{FxHashMap, FxHashSet};
use typst::syntax::ast::AstNode;
use typst::syntax::{Span, SyntaxKind, SyntaxNode, ast};

#[cfg(test)]
mod tests;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BodyId(pub usize);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BodyKind {
    Root,
    Closure,
}

#[derive(Debug, Clone)]
pub struct CfgCollection {
    pub bodies: Vec<ControlFlowGraph>,
}

impl CfgCollection {
    pub fn body(&self, id: BodyId) -> &ControlFlowGraph {
        &self.bodies[id.0]
    }
}

#[derive(Debug, Clone)]
pub struct ControlFlowGraph {
    pub id: BodyId,
    pub kind: BodyKind,
    pub origin: Span,

    pub entry: BlockId,
    pub exit: BlockId,
    pub error_exit: BlockId,

    pub blocks: Vec<BasicBlock>,
}

impl ControlFlowGraph {
    pub fn block(&self, id: BlockId) -> &BasicBlock {
        &self.blocks[id.0]
    }

    pub fn successors(&self, id: BlockId) -> [Option<BlockId>; 2] {
        self.block(id).terminator.successors()
    }

    pub fn predecessors(&self) -> Vec<Vec<BlockId>> {
        let mut preds: Vec<Vec<BlockId>> = vec![Vec::new(); self.blocks.len()];
        for from in 0..self.blocks.len() {
            let from = BlockId(from);
            for succ in self.successors(from).into_iter().flatten() {
                preds[succ.0].push(from);
            }
        }
        preds
    }

    pub fn reachable_blocks(&self) -> FxHashSet<BlockId> {
        let mut seen: FxHashSet<BlockId> = FxHashSet::default();
        let mut stack = vec![self.entry];
        while let Some(bb) = stack.pop() {
            if !seen.insert(bb) {
                continue;
            }
            for succ in self.successors(bb).into_iter().flatten() {
                stack.push(succ);
            }
        }
        seen
    }

    /// Basic debug dump that stays stable enough for snapshot tests.
    pub fn debug_dump(&self) -> String {
        use core::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(
            &mut out,
            "Body {:?} origin={:?} entry={:?} exit={:?} error_exit={:?}",
            self.kind, self.origin, self.entry, self.exit, self.error_exit
        );
        for (i, bb) in self.blocks.iter().enumerate() {
            let _ = writeln!(
                &mut out,
                "  bb{:#?}: stmts={} term={:?}",
                BlockId(i),
                bb.stmts.len(),
                bb.terminator
            );
        }
        out
    }
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub stmts: Vec<Stmt>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub span: Span,
    pub kind: SyntaxKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExitKind {
    Normal,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BranchKind {
    If,
    While,
    ForIter,
    And,
    Or,
}

#[derive(Debug, Clone)]
pub enum Terminator {
    Unset,
    Exit(ExitKind),
    Goto(BlockId),
    Branch {
        kind: BranchKind,
        span: Span,
        then_bb: BlockId,
        else_bb: BlockId,
    },
    Return {
        span: Span,
        target: BlockId,
        allowed: bool,
    },
    Break {
        span: Span,
        target: BlockId,
        allowed: bool,
    },
    Continue {
        span: Span,
        target: BlockId,
        allowed: bool,
    },
}

impl Terminator {
    pub fn successors(&self) -> [Option<BlockId>; 2] {
        match *self {
            Terminator::Unset | Terminator::Exit(..) => [None, None],
            Terminator::Goto(bb) => [Some(bb), None],
            Terminator::Branch {
                then_bb, else_bb, ..
            } => [Some(then_bb), Some(else_bb)],
            Terminator::Return { target, .. }
            | Terminator::Break { target, .. }
            | Terminator::Continue { target, .. } => [Some(target), None],
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct LoopTargets {
    break_target: BlockId,
    continue_target: BlockId,
}

#[derive(Debug, Clone, Copy)]
struct ReturnPolicy {
    allowed: bool,
    target: BlockId,
}

#[derive(Debug, Clone)]
struct BuildCtx {
    loops: Vec<LoopTargets>,
    ret: ReturnPolicy,
    error_exit: BlockId,
}

struct CollectionBuilder {
    bodies: Vec<ControlFlowGraph>,
}

impl CollectionBuilder {
    fn new() -> Self {
        Self { bodies: Vec::new() }
    }

    fn push_body(&mut self, mut cfg: ControlFlowGraph) -> BodyId {
        let id = BodyId(self.bodies.len());
        cfg.id = id;
        self.bodies.push(cfg);
        id
    }

    fn build_root<'a>(&mut self, root: ast::Markup<'a>) -> BodyId {
        self.build_body_from_exprs(BodyKind::Root, root.span(), root.exprs(), false)
    }

    fn build_closure<'a>(&mut self, closure: ast::Closure<'a>) -> BodyId {
        self.build_body_from_expr(BodyKind::Closure, closure.span(), closure.body(), true)
    }

    fn build_body_from_exprs<'a>(
        &mut self,
        kind: BodyKind,
        origin: Span,
        exprs: impl Iterator<Item = ast::Expr<'a>>,
        allow_return: bool,
    ) -> BodyId {
        let mut builder = BodyBuilder::new(kind, origin, allow_return);
        for expr in exprs {
            builder.eval_expr(expr, self);
        }
        self.push_body(builder.finish())
    }

    fn build_body_from_expr<'a>(
        &mut self,
        kind: BodyKind,
        origin: Span,
        expr: ast::Expr<'a>,
        allow_return: bool,
    ) -> BodyId {
        let mut builder = BodyBuilder::new(kind, origin, allow_return);
        builder.eval_expr(expr, self);
        self.push_body(builder.finish())
    }
}

struct BodyBuilder {
    kind: BodyKind,
    origin: Span,
    blocks: Vec<BasicBlock>,
    entry: BlockId,
    exit: BlockId,
    error_exit: BlockId,
    current: Option<BlockId>,
    ctx: BuildCtx,
}

impl BodyBuilder {
    fn new(kind: BodyKind, origin: Span, allow_return: bool) -> Self {
        let mut blocks = Vec::new();
        let entry = BlockId(blocks.len());
        blocks.push(BasicBlock {
            stmts: Vec::new(),
            terminator: Terminator::Unset,
        });
        let exit = BlockId(blocks.len());
        blocks.push(BasicBlock {
            stmts: Vec::new(),
            terminator: Terminator::Exit(ExitKind::Normal),
        });
        let error_exit = BlockId(blocks.len());
        blocks.push(BasicBlock {
            stmts: Vec::new(),
            terminator: Terminator::Exit(ExitKind::Error),
        });

        Self {
            kind,
            origin,
            blocks,
            entry,
            exit,
            error_exit,
            current: Some(entry),
            ctx: BuildCtx {
                loops: Vec::new(),
                ret: ReturnPolicy {
                    allowed: allow_return,
                    target: if allow_return { exit } else { error_exit },
                },
                error_exit,
            },
        }
    }

    fn finish(mut self) -> ControlFlowGraph {
        if let Some(bb) = self.current.take() {
            if matches!(self.blocks[bb.0].terminator, Terminator::Unset) {
                self.blocks[bb.0].terminator = Terminator::Goto(self.exit);
            }
        }

        ControlFlowGraph {
            id: BodyId(usize::MAX),
            kind: self.kind,
            origin: self.origin,
            entry: self.entry,
            exit: self.exit,
            error_exit: self.error_exit,
            blocks: self.blocks,
        }
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len());
        self.blocks.push(BasicBlock {
            stmts: Vec::new(),
            terminator: Terminator::Unset,
        });
        id
    }

    fn ensure_current(&mut self) -> BlockId {
        if let Some(bb) = self.current {
            return bb;
        }
        let bb = self.new_block();
        self.current = Some(bb);
        bb
    }

    fn set_terminator(&mut self, bb: BlockId, term: Terminator) {
        let slot = &mut self.blocks[bb.0].terminator;
        debug_assert!(matches!(slot, Terminator::Unset));
        *slot = term;
    }

    fn append_stmt(&mut self, span: Span, kind: SyntaxKind) {
        let bb = self.ensure_current();
        self.blocks[bb.0].stmts.push(Stmt { span, kind });
    }

    fn eval_untyped_children<'a>(&mut self, node: &'a SyntaxNode, col: &mut CollectionBuilder) {
        for child in node.children() {
            if let Some(expr) = child.cast::<ast::Expr<'a>>() {
                self.eval_expr(expr, col);
            } else {
                self.eval_untyped_children(&child, col);
            }
            if self.current.is_none() {
                // Keep walking to record unreachable blocks/statements, but
                // avoid accidentally connecting control-flow.
                self.ensure_current();
            }
        }
    }

    fn eval_expr<'a>(&mut self, expr: ast::Expr<'a>, col: &mut CollectionBuilder) {
        match expr {
            ast::Expr::Conditional(cond) => {
                let cond_span = cond.condition().span();

                self.eval_expr(cond.condition(), col);
                let Some(head) = self.current else {
                    return;
                };

                let then_bb = self.new_block();
                let else_bb = self.new_block();
                let join_bb = self.new_block();

                self.set_terminator(
                    head,
                    Terminator::Branch {
                        kind: BranchKind::If,
                        span: cond_span,
                        then_bb,
                        else_bb,
                    },
                );
                self.current = None;

                // then
                self.current = Some(then_bb);
                self.eval_expr(cond.if_body(), col);
                if let Some(end) = self.current.take() {
                    if matches!(self.blocks[end.0].terminator, Terminator::Unset) {
                        self.set_terminator(end, Terminator::Goto(join_bb));
                    }
                }

                // else
                self.current = Some(else_bb);
                if let Some(else_body) = cond.else_body() {
                    self.eval_expr(else_body, col);
                }
                if let Some(end) = self.current.take() {
                    if matches!(self.blocks[end.0].terminator, Terminator::Unset) {
                        self.set_terminator(end, Terminator::Goto(join_bb));
                    }
                }

                self.current = Some(join_bb);
                self.append_stmt(expr.span(), SyntaxKind::Conditional);
            }

            ast::Expr::WhileLoop(w) => {
                let before = self.ensure_current();
                let header = self.new_block();
                let body = self.new_block();
                let exit = self.new_block();

                if matches!(self.blocks[before.0].terminator, Terminator::Unset) {
                    self.set_terminator(before, Terminator::Goto(header));
                }

                // header
                self.current = Some(header);
                let cond_span = w.condition().span();
                self.eval_expr(w.condition(), col);
                let Some(head_end) = self.current else {
                    return;
                };
                self.set_terminator(
                    head_end,
                    Terminator::Branch {
                        kind: BranchKind::While,
                        span: cond_span,
                        then_bb: body,
                        else_bb: exit,
                    },
                );
                self.current = None;

                // body
                let old_loops_len = self.ctx.loops.len();
                self.ctx.loops.push(LoopTargets {
                    break_target: exit,
                    continue_target: header,
                });
                self.current = Some(body);
                self.eval_expr(w.body(), col);
                self.ctx.loops.truncate(old_loops_len);

                if let Some(body_end) = self.current.take() {
                    if matches!(self.blocks[body_end.0].terminator, Terminator::Unset) {
                        self.set_terminator(body_end, Terminator::Goto(header));
                    }
                }

                self.current = Some(exit);
                self.append_stmt(expr.span(), SyntaxKind::WhileLoop);
            }

            ast::Expr::ForLoop(f) => {
                // Evaluate iterable first.
                self.eval_expr(f.iterable(), col);
                let Some(iter_end) = self.current else {
                    return;
                };

                let header = self.new_block();
                let body = self.new_block();
                let exit = self.new_block();

                if matches!(self.blocks[iter_end.0].terminator, Terminator::Unset) {
                    self.set_terminator(iter_end, Terminator::Goto(header));
                }

                // header (iteration step / next)
                self.current = Some(header);
                self.append_stmt(f.span(), SyntaxKind::ForLoop);
                self.set_terminator(
                    header,
                    Terminator::Branch {
                        kind: BranchKind::ForIter,
                        span: f.span(),
                        then_bb: body,
                        else_bb: exit,
                    },
                );
                self.current = None;

                // body
                let old_loops_len = self.ctx.loops.len();
                self.ctx.loops.push(LoopTargets {
                    break_target: exit,
                    continue_target: header,
                });
                self.current = Some(body);
                self.eval_expr(f.body(), col);
                self.ctx.loops.truncate(old_loops_len);

                if let Some(body_end) = self.current.take() {
                    if matches!(self.blocks[body_end.0].terminator, Terminator::Unset) {
                        self.set_terminator(body_end, Terminator::Goto(header));
                    }
                }

                self.current = Some(exit);
            }

            ast::Expr::LoopBreak(_) => {
                self.append_stmt(expr.span(), SyntaxKind::LoopBreak);
                let (target, allowed) = if let Some(loop_) = self.ctx.loops.last() {
                    (loop_.break_target, true)
                } else {
                    (self.ctx.error_exit, false)
                };
                let bb = self.ensure_current();
                self.set_terminator(
                    bb,
                    Terminator::Break {
                        span: expr.span(),
                        target,
                        allowed,
                    },
                );
                self.current = None;
            }

            ast::Expr::LoopContinue(_) => {
                self.append_stmt(expr.span(), SyntaxKind::LoopContinue);
                let (target, allowed) = if let Some(loop_) = self.ctx.loops.last() {
                    (loop_.continue_target, true)
                } else {
                    (self.ctx.error_exit, false)
                };
                let bb = self.ensure_current();
                self.set_terminator(
                    bb,
                    Terminator::Continue {
                        span: expr.span(),
                        target,
                        allowed,
                    },
                );
                self.current = None;
            }

            ast::Expr::FuncReturn(ret) => {
                if let Some(body) = ret.body() {
                    self.eval_expr(body, col);
                }
                self.append_stmt(expr.span(), SyntaxKind::FuncReturn);
                let bb = self.ensure_current();
                self.set_terminator(
                    bb,
                    Terminator::Return {
                        span: expr.span(),
                        target: self.ctx.ret.target,
                        allowed: self.ctx.ret.allowed,
                    },
                );
                self.current = None;
            }

            ast::Expr::Contextual(ctx_expr) => {
                // Contextual expressions act like a "return boundary": `return`
                // exits the contextual expression, not the surrounding body.
                let before = self.ensure_current();
                let body_entry = self.new_block();
                let after = self.new_block();
                if matches!(self.blocks[before.0].terminator, Terminator::Unset) {
                    self.set_terminator(before, Terminator::Goto(body_entry));
                }

                let saved = self.ctx.ret;
                self.ctx.ret = ReturnPolicy {
                    allowed: true,
                    target: after,
                };

                self.current = Some(body_entry);
                self.eval_expr(ctx_expr.body(), col);

                self.ctx.ret = saved;

                if let Some(end) = self.current.take() {
                    if matches!(self.blocks[end.0].terminator, Terminator::Unset) {
                        self.set_terminator(end, Terminator::Goto(after));
                    }
                }
                self.current = Some(after);
                self.append_stmt(expr.span(), SyntaxKind::Contextual);
            }

            ast::Expr::Binary(bin) if matches!(bin.op(), ast::BinOp::And | ast::BinOp::Or) => {
                let span = expr.span();
                let op = bin.op();

                self.eval_expr(bin.lhs(), col);
                let Some(head) = self.current else {
                    return;
                };

                let rhs_bb = self.new_block();
                let join_bb = self.new_block();

                let (then_bb, else_bb, kind) = match op {
                    ast::BinOp::And => (rhs_bb, join_bb, BranchKind::And),
                    ast::BinOp::Or => (join_bb, rhs_bb, BranchKind::Or),
                    _ => unreachable!(),
                };

                self.set_terminator(
                    head,
                    Terminator::Branch {
                        kind,
                        span,
                        then_bb,
                        else_bb,
                    },
                );
                self.current = None;

                self.current = Some(rhs_bb);
                self.eval_expr(bin.rhs(), col);
                if let Some(end) = self.current.take() {
                    if matches!(self.blocks[end.0].terminator, Terminator::Unset) {
                        self.set_terminator(end, Terminator::Goto(join_bb));
                    }
                }

                self.current = Some(join_bb);
                self.append_stmt(span, SyntaxKind::Binary);
            }

            ast::Expr::Closure(closure) => {
                // The closure's body is not executed here, but we still build a
                // separate CFG for it.
                col.build_closure(closure);
                self.append_stmt(expr.span(), SyntaxKind::Closure);
            }

            _ => {
                self.eval_untyped_children(expr.to_untyped(), col);
                self.append_stmt(expr.span(), expr.to_untyped().kind());
            }
        }
    }
}

/// Builds CFGs for the file root (and nested closures).
pub fn build_cfgs(root: &SyntaxNode) -> CfgCollection {
    let Some(markup) = root.cast::<ast::Markup>() else {
        return CfgCollection { bodies: Vec::new() };
    };

    let mut builder = CollectionBuilder::new();
    let _root_id = builder.build_root(markup);
    CfgCollection {
        bodies: builder.bodies,
    }
}

/// Returns blocks that are structurally unreachable because the builder had no
/// incoming edges for them (typically code after `return`/`break`/`continue`).
pub fn orphan_blocks(cfg: &ControlFlowGraph) -> Vec<BlockId> {
    let preds = cfg.predecessors();
    (0..cfg.blocks.len())
        .map(BlockId)
        .filter(|&bb| {
            bb != cfg.entry && bb != cfg.exit && bb != cfg.error_exit && preds[bb.0].is_empty()
        })
        .collect()
}

/// Returns a best-effort mapping from statement spans to blocks.
pub fn stmt_index(cfg: &ControlFlowGraph) -> FxHashMap<Span, BlockId> {
    let mut map = FxHashMap::default();
    for (bb_idx, bb) in cfg.blocks.iter().enumerate() {
        let bb_id = BlockId(bb_idx);
        for stmt in &bb.stmts {
            map.entry(stmt.span).or_insert(bb_id);
        }
    }
    map
}

#[derive(Debug, Clone)]
pub struct Dominators {
    /// Immediate dominator for each block (or `None` if unreachable).
    pub idom: Vec<Option<BlockId>>,
    /// Reverse postorder of reachable blocks.
    pub rpo: Vec<BlockId>,
}

impl Dominators {
    pub fn dominates(&self, a: BlockId, mut b: BlockId) -> bool {
        if a == b {
            return true;
        }
        while let Some(idom) = self.idom.get(b.0).and_then(|v| *v) {
            if idom == a {
                return true;
            }
            if idom == b {
                break;
            }
            b = idom;
        }
        false
    }
}

pub fn dominators(cfg: &ControlFlowGraph) -> Dominators {
    let preds = cfg.predecessors();
    let reachable = cfg.reachable_blocks();

    // Reverse postorder numbering.
    fn dfs(
        cfg: &ControlFlowGraph,
        reachable: &FxHashSet<BlockId>,
        bb: BlockId,
        seen: &mut FxHashSet<BlockId>,
        post: &mut Vec<BlockId>,
    ) {
        if !reachable.contains(&bb) || !seen.insert(bb) {
            return;
        }
        for succ in cfg.successors(bb).into_iter().flatten() {
            dfs(cfg, reachable, succ, seen, post);
        }
        post.push(bb);
    }

    let mut post = Vec::new();
    dfs(
        cfg,
        &reachable,
        cfg.entry,
        &mut FxHashSet::default(),
        &mut post,
    );
    let mut rpo = post;
    rpo.reverse();

    let mut rpo_index: Vec<Option<usize>> = vec![None; cfg.blocks.len()];
    for (i, bb) in rpo.iter().enumerate() {
        rpo_index[bb.0] = Some(i);
    }

    let mut idom: Vec<Option<BlockId>> = vec![None; cfg.blocks.len()];
    idom[cfg.entry.0] = Some(cfg.entry);

    let intersect = |idom: &Vec<Option<BlockId>>,
                     rpo_index: &Vec<Option<usize>>,
                     mut f1: BlockId,
                     mut f2: BlockId|
     -> BlockId {
        while f1 != f2 {
            while rpo_index[f1.0].unwrap_or(usize::MAX) > rpo_index[f2.0].unwrap_or(usize::MAX) {
                f1 = idom[f1.0].unwrap();
            }
            while rpo_index[f2.0].unwrap_or(usize::MAX) > rpo_index[f1.0].unwrap_or(usize::MAX) {
                f2 = idom[f2.0].unwrap();
            }
        }
        f1
    };

    let mut changed = true;
    while changed {
        changed = false;
        for &b in rpo.iter().skip(1) {
            let mut new_idom: Option<BlockId> = None;
            for &p in &preds[b.0] {
                if !reachable.contains(&p) {
                    continue;
                }
                if idom[p.0].is_none() {
                    continue;
                }
                new_idom = Some(match new_idom {
                    None => p,
                    Some(q) => intersect(&idom, &rpo_index, p, q),
                });
            }
            if idom[b.0] != new_idom {
                idom[b.0] = new_idom;
                changed = true;
            }
        }
    }

    Dominators { idom, rpo }
}

pub fn back_edges(cfg: &ControlFlowGraph, dom: &Dominators) -> Vec<(BlockId, BlockId)> {
    let mut edges = Vec::new();
    for from in 0..cfg.blocks.len() {
        let from = BlockId(from);
        for to in cfg.successors(from).into_iter().flatten() {
            if dom.dominates(to, from) {
                edges.push((from, to));
            }
        }
    }
    edges
}

pub fn natural_loop(cfg: &ControlFlowGraph, header: BlockId, back: BlockId) -> FxHashSet<BlockId> {
    let preds = cfg.predecessors();
    let mut set: FxHashSet<BlockId> = FxHashSet::default();
    set.insert(header);
    set.insert(back);
    let mut stack = vec![back];
    while let Some(n) = stack.pop() {
        for &p in &preds[n.0] {
            if set.insert(p) {
                stack.push(p);
            }
        }
    }
    set
}
