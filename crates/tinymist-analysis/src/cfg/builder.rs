use rustc_hash::FxHashMap;
use typst::syntax::ast::AstNode;
use typst::syntax::{Span, SyntaxKind, SyntaxNode, ast};

use super::ir::*;

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
    closure_bodies: FxHashMap<Span, BodyId>,
    decl_bodies: FxHashMap<Span, BodyId>,
}

impl CollectionBuilder {
    fn new() -> Self {
        Self {
            bodies: Vec::new(),
            closure_bodies: FxHashMap::default(),
            decl_bodies: FxHashMap::default(),
        }
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
        let id = self.build_body_from_expr(BodyKind::Closure, closure.span(), closure.body(), true);
        self.closure_bodies.insert(closure.span(), id);
        id
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
        if let Some(bb) = self.current.take()
            && matches!(self.blocks[bb.0].terminator, Terminator::Unset)
        {
            self.blocks[bb.0].terminator = Terminator::Goto(self.exit);
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
                self.eval_untyped_children(child, col);
            }
        }
    }

    fn eval_expr<'a>(&mut self, expr: ast::Expr<'a>, col: &mut CollectionBuilder) {
        match expr {
            ast::Expr::CodeBlock(code_block) => {
                for e in code_block.body().exprs() {
                    self.eval_expr(e, col);
                }
            }

            ast::Expr::Parenthesized(paren) => {
                self.eval_expr(paren.expr(), col);
            }

            ast::Expr::Conditional(cond) => {
                let cond_expr = cond.condition();
                let cond_span = cond_expr.span();
                let cond_const = const_bool(cond_expr);

                self.eval_expr(cond_expr, col);
                let Some(head) = self.current else {
                    return;
                };

                let then_bb = self.new_block();
                let else_bb = self.new_block();
                let join_bb = self.new_block();

                match cond_const {
                    Some(true) => self.set_terminator(head, Terminator::Goto(then_bb)),
                    Some(false) => self.set_terminator(head, Terminator::Goto(else_bb)),
                    None => self.set_terminator(
                        head,
                        Terminator::Branch {
                            kind: BranchKind::If,
                            span: cond_span,
                            then_bb,
                            else_bb,
                        },
                    ),
                }
                self.current = None;

                // then
                self.current = Some(then_bb);
                self.eval_expr(cond.if_body(), col);
                if let Some(end) = self.current.take()
                    && matches!(self.blocks[end.0].terminator, Terminator::Unset)
                {
                    self.set_terminator(end, Terminator::Goto(join_bb));
                }

                // else
                self.current = Some(else_bb);
                if let Some(else_body) = cond.else_body() {
                    self.eval_expr(else_body, col);
                }
                if let Some(end) = self.current.take()
                    && matches!(self.blocks[end.0].terminator, Terminator::Unset)
                {
                    self.set_terminator(end, Terminator::Goto(join_bb));
                }

                self.current = Some(join_bb);
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

                if let Some(body_end) = self.current.take()
                    && matches!(self.blocks[body_end.0].terminator, Terminator::Unset)
                {
                    self.set_terminator(body_end, Terminator::Goto(header));
                }

                self.current = Some(exit);
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

                if let Some(body_end) = self.current.take()
                    && matches!(self.blocks[body_end.0].terminator, Terminator::Unset)
                {
                    self.set_terminator(body_end, Terminator::Goto(header));
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
                if !allowed {
                    return;
                }
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
                if !allowed {
                    return;
                }
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
                if !self.ctx.ret.allowed {
                    return;
                }
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

            ast::Expr::LetBinding(let_) => {
                // Record the let binding as a statement in the current body.
                self.append_stmt(expr.span(), SyntaxKind::LetBinding);

                // If this is a closure-valued binding, build a separate CFG for
                // the closure and remember the declaration -> body mapping so
                // interprocedural analyses can resolve calls.
                if let Some(ast::Expr::Closure(closure)) = let_.init() {
                    let body_id = col.build_closure(closure);

                    match let_.kind() {
                        ast::LetBindingKind::Closure(ident) => {
                            col.decl_bodies.insert(ident.span(), body_id);
                        }
                        ast::LetBindingKind::Normal(pattern) => {
                            // Best-effort: only handle `let f = (..) => ..`.
                            if let ast::Pattern::Normal(ast::Expr::Ident(ident)) = pattern {
                                col.decl_bodies.insert(ident.span(), body_id);
                            }
                        }
                    }

                    // Do not descend into the closure: its body isn't executed
                    // at binding time and is represented by the separate CFG.
                    return;
                }

                // Otherwise, descend into children for best-effort control flow.
                self.eval_untyped_children(expr.to_untyped(), col);
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

                if let Some(end) = self.current.take()
                    && matches!(self.blocks[end.0].terminator, Terminator::Unset)
                {
                    self.set_terminator(end, Terminator::Goto(after));
                }
                self.current = Some(after);
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
                if let Some(end) = self.current.take()
                    && matches!(self.blocks[end.0].terminator, Terminator::Unset)
                {
                    self.set_terminator(end, Terminator::Goto(join_bb));
                }

                self.current = Some(join_bb);
            }

            ast::Expr::Closure(closure) => {
                // The closure's body is not executed here, but we still build a
                // separate CFG for it.
                col.build_closure(closure);
                self.append_stmt(expr.span(), SyntaxKind::Closure);
            }

            _ => {
                // Record the statement before descending: some expression kinds
                // (e.g. content blocks / code injections) contain `return`/`break`
                // as children, and visiting children first would incorrectly make
                // the container expression appear "after" the terminator.
                let untyped = expr.to_untyped();
                self.append_stmt(expr.span(), untyped.kind());
                self.eval_untyped_children(untyped, col);
            }
        }
    }
}

fn const_bool(expr: ast::Expr<'_>) -> Option<bool> {
    match expr {
        ast::Expr::Bool(b) => Some(b.get()),
        ast::Expr::Parenthesized(p) => const_bool(p.expr()),
        ast::Expr::Unary(u) => match u.op() {
            ast::UnOp::Not => const_bool(u.expr()).map(|v| !v),
            _ => None,
        },
        ast::Expr::Binary(b) => match b.op() {
            ast::BinOp::And => Some(const_bool(b.lhs())? && const_bool(b.rhs())?),
            ast::BinOp::Or => Some(const_bool(b.lhs())? || const_bool(b.rhs())?),
            _ => None,
        },
        _ => None,
    }
}

/// Builds CFGs for the file root (and nested closures).
pub fn build_cfgs(root: &SyntaxNode) -> CfgCollection {
    build_cfgs_many(std::iter::once(root))
}

/// Builds CFGs for multiple file roots (and all their nested closures).
///
/// This is useful for building a project-wide CFG collection, where
/// declarations and call edges may resolve across files via spans (which embed
/// their file ids).
pub fn build_cfgs_many<'a>(roots: impl IntoIterator<Item = &'a SyntaxNode>) -> CfgCollection {
    let mut builder = CollectionBuilder::new();
    for root in roots {
        let Some(markup) = root.cast::<ast::Markup>() else {
            continue;
        };
        let _root_id = builder.build_root(markup);
    }

    CfgCollection {
        bodies: builder.bodies,
        closure_bodies: builder.closure_bodies,
        decl_bodies: builder.decl_bodies,
    }
}
