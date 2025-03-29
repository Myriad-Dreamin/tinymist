//! Control flow graph construction.
//!
//! A document has a root region. The function each also
//! has a sub region.
//!
//! Every region has a list of basic blocks. Each basic block
//! references a list of nodes.

use std::sync::{Arc, OnceLock};

use rustc_hash::FxHashMap;
use typst::syntax::Span;

use super::def::*;
use super::CfInfo;
use crate::adt::IndexVec;
use crate::analysis::SharedContext;
use crate::syntax::{ArgExpr, Decl, Expr, ExprInfo};
use crate::ty::{Interned, Ty};

/// Converts an expression into a control flow graph.
pub(crate) fn control_flow_of(ctx: Arc<SharedContext>, ei: Arc<ExprInfo>) -> Arc<CfInfo> {
    let mut cf = CfInfo::new();
    let region = cf.root;

    // let region = cf.region.create();
    // let block = region.basic_blocks.create();
    // let region = region.id;
    // let block = block.id;

    let mut worker = CfWorker {
        ctx: ctx.clone(),
        ei: ei.clone(),
        cf: &mut cf,
        rg: RegionBuilder {
            region,
            nodes: Vec::new(),
            loop_label: None,
            cont: None,
        },
        decls: FxHashMap::default(),
    };

    // , &mut [].iter()
    worker.expr(&ei.root);

    Arc::new(cf)
}

struct RegionBuilder {
    region: RegionId,
    nodes: Vec<NodeId>,
    cont: Option<BasicBlockId>,
    loop_label: Option<BasicBlockId>,
}

struct CfWorker<'a> {
    ctx: Arc<SharedContext>,
    ei: Arc<ExprInfo>,
    cf: &'a mut CfInfo,
    rg: RegionBuilder,
    decls: FxHashMap<Interned<Decl>, NodeId>,
}

impl CfWorker<'_> {
    fn this_region(&mut self) -> &mut Region {
        self.cf.region.get_mut(self.rg.region)
    }

    fn create_block(&mut self, nodes: Vec<NodeId>) -> BasicBlockId {
        let bb = self.this_region().basic_blocks.create();
        bb.nodes = nodes;
        bb.id
    }

    fn create_node(&mut self, node: CfNode) -> NodeId {
        let node = self.this_region().nodes.push(node);
        self.rg.nodes.push(node);
        node
    }

    fn create_cont(&mut self) -> BasicBlockId {
        *self.rg.cont.get_or_insert_with(|| {
            let bb = self.cf.region.get_mut(self.rg.region).basic_blocks.create();
            bb.id
        })
    }

    fn block<'a>(&mut self, cont: impl Iterator<Item = &'a Expr>) -> BasicBlockId {
        self.block_with(cont, Vec::new())
    }

    fn block_with<'a>(
        &mut self,
        cont: impl Iterator<Item = &'a Expr>,
        nodes: Vec<NodeId>,
    ) -> BasicBlockId {
        let block = self.create_block(nodes);
        self.cont_to(cont, block);
        block
    }

    fn cont_to<'a>(
        &mut self,
        mut cont: impl Iterator<Item = &'a Expr>,
        this: BasicBlockId,
    ) -> BasicBlockId {
        let parent_nodes = std::mem::take(&mut self.rg.nodes);

        while let Some(expr) = cont.next() {
            self.expr_ins(expr);
            if let Some(next) = self.rg.cont.take() {
                self.create_node(CfNode::detached(CfInstr::Branch(next)));
                self.cont_to(cont, next);
                break;
            }
        }

        let nodes = std::mem::replace(&mut self.rg.nodes, parent_nodes);

        let basic_block = self
            .cf
            .region
            .get_mut(self.rg.region)
            .basic_blocks
            .get_mut(this);
        if !basic_block.nodes.is_empty() {
            let mut nodes = nodes;
            basic_block.nodes.append(&mut nodes);
        } else {
            basic_block.nodes = nodes;
        }
        this
    }

    fn expr_ins(&mut self, expr: &Expr) -> NodeId {
        let expr = self.expr(expr);
        self.create_node(expr)
    }

    fn expr(&mut self, expr: &Expr) -> CfNode {
        match expr {
            Expr::Block(interned) => {
                let body = self.block(&mut interned.iter());
                CfNode::detached(CfInstr::Block(CfBlock { ty: Ty::Any, body }))
            }
            Expr::Element(interned) => {
                let body = self.block(&mut interned.content.iter());
                CfNode::detached(CfInstr::Element(CfElement {
                    elem: interned.elem,
                    body,
                }))
            }
            Expr::Cov(cov) => {
                let first = self
                    .this_region()
                    .nodes
                    .push(CfNode::detached(CfInstr::CovPoint(cov.first)));
                let last = self
                    .this_region()
                    .nodes
                    .push(CfNode::detached(CfInstr::CovPoint(cov.last)));
                let body = self.block_with(&mut [&cov.body].into_iter(), vec![first]);
                let bb = self.this_region().basic_blocks.get_mut(body);
                bb.nodes.push(last);

                CfNode::detached(CfInstr::Block(CfBlock { ty: Ty::Any, body }))
            }
            Expr::Conditional(interned) => {
                let cond = self.expr_ins(&interned.cond);
                let then = Box::new(self.expr(&interned.then));
                let else_ = Box::new(self.expr(&interned.else_));

                CfNode::detached(CfInstr::If(CfIf {
                    ty: Ty::Any,
                    cond,
                    then,
                    else_,
                }))
            }
            Expr::Array(interned) => {
                let args = interned.args.iter().map(|arg| self.arg(arg)).collect();

                CfNode::detached(CfInstr::Array(CfArgs { ty: Ty::Any, args }))
            }
            Expr::Dict(interned) => {
                let args = interned.args.iter().map(|arg| self.arg(arg)).collect();

                CfNode::detached(CfInstr::Dict(CfArgs { ty: Ty::Any, args }))
            }
            Expr::Args(interned) => {
                let args = interned.args.iter().map(|arg| self.arg(arg)).collect();

                CfNode::detached(CfInstr::Args(CfArgs { ty: Ty::Any, args }))
            }
            Expr::Unary(interned) => {
                let arg = self.expr_ins(&interned.lhs);
                CfNode::detached(CfInstr::Un(CfUn {
                    ty: Ty::Any,
                    op: interned.op,
                    lhs: arg,
                }))
            }
            Expr::Binary(interned) => {
                let (lhs, rhs) = &interned.operands;
                let lhs = self.expr_ins(lhs);
                let rhs = self.expr_ins(rhs);
                CfNode::detached(CfInstr::Bin(CfBin {
                    ty: Ty::Any,
                    op: interned.op,
                    lhs,
                    rhs,
                }))
            }
            Expr::Apply(interned) => {
                let func = self.expr_ins(&interned.callee);
                let args = self.expr_ins(&interned.args);

                CfNode::detached(CfInstr::Apply(CfCall {
                    ty: Ty::Any,
                    func,
                    args,
                }))
            }
            Expr::Func(interned) => {
                let checkpoint = self.create_region();
                // todo: destruct
                let body = self.expr(&interned.body);
                let body = self.rg.region;
                self.restore_region(checkpoint);

                let func = CfNode::detached(CfInstr::Func(CfFunc { ty: Ty::Any, body }));

                let idx = self.create_node(func.clone());
                self.declare(&interned.decl, idx);
                func
            }
            Expr::Let(interned) => {
                let init = interned.body.as_ref().map(|expr| self.expr_ins(expr));
                CfNode::detached(CfInstr::Let(CfLet {
                    ty: Ty::Any,
                    pattern: interned.pattern.clone(),
                    init,
                }))
            }
            Expr::Show(interned) => {
                let selector = interned.selector.as_ref().map(|expr| self.expr_ins(expr));
                let edit = self.expr_ins(&interned.edit);
                let cont = self.create_cont();

                CfNode::detached(CfInstr::Show(CfShow {
                    selector,
                    edit,
                    cont,
                }))
            }
            Expr::Set(interned) => {
                // pub target: Expr,
                // pub args: Expr,
                // pub cond: Option<Expr>,

                let target = self.expr_ins(&interned.target);
                let args = self.expr_ins(&interned.args);
                let cond = interned.cond.as_ref().map(|expr| self.expr_ins(expr));
                let cont = self.create_cont();

                let set_cont = CfNode::detached(CfInstr::Set(CfSet { target, args, cont }));

                if let Some(cond) = cond {
                    let no_set_cont = Box::new(CfNode::detached(CfInstr::Branch(cont)));
                    CfNode::detached(CfInstr::If(CfIf {
                        ty: Ty::Any,
                        cond,
                        then: Box::new(set_cont),
                        else_: no_set_cont,
                    }))
                } else {
                    set_cont
                }
            }
            Expr::Import(interned) => CfNode::detached(CfInstr::Meta(expr.clone())),
            Expr::Include(interned) => {
                let source = self.expr_ins(&interned.source);
                CfNode::detached(CfInstr::Include(source))
            }
            Expr::Select(interned) => {
                let lhs = self.expr_ins(&interned.lhs);
                CfNode::detached(CfInstr::Select(CfSelect {
                    ty: Ty::Any,
                    lhs,
                    key: interned.key.clone(),
                }))
            }
            Expr::Contextual(interned) => {
                let bb = self.block([interned.as_ref()].into_iter());
                CfNode::detached(CfInstr::Contextual(bb))
            }
            Expr::ForLoop(interned) => {
                let iter = self.expr_ins(&interned.iter);
                let iter = self.create_node(CfNode::detached(CfInstr::Iter(iter)));

                let loop_cont = self.create_cont();
                let loop_label = self.create_block(Vec::new());
                let parent_loop_label = self.rg.loop_label.replace(loop_label);

                let parent_nodes = std::mem::take(&mut self.rg.nodes);

                // todo: destruct
                let _ = iter;
                self.expr(&interned.body);

                self.rg.loop_label = parent_loop_label;
                let nodes = std::mem::replace(&mut self.rg.nodes, parent_nodes);
                self.cf
                    .region
                    .get_mut(self.rg.region)
                    .basic_blocks
                    .get_mut(loop_label)
                    .nodes = nodes;

                CfNode::detached(CfInstr::Loop(CfLoop {
                    ty: Ty::Any,
                    cond: iter,
                    body: loop_label,
                    cont: Some(loop_cont),
                }))
            }
            Expr::WhileLoop(interned) => {
                let cond = self.expr_ins(&interned.cond);

                let loop_cont = self.create_cont();
                let loop_label = self.create_block(Vec::new());
                let parent_loop_label = self.rg.loop_label.replace(loop_label);

                let parent_nodes = std::mem::take(&mut self.rg.nodes);
                self.expr(&interned.body);

                self.rg.loop_label = parent_loop_label;
                let nodes = std::mem::replace(&mut self.rg.nodes, parent_nodes);
                self.cf
                    .region
                    .get_mut(self.rg.region)
                    .basic_blocks
                    .get_mut(loop_label)
                    .nodes = nodes;

                CfNode::detached(CfInstr::Loop(CfLoop {
                    ty: Ty::Any,
                    cond,
                    body: loop_label,
                    cont: Some(loop_cont),
                }))
            }
            Expr::Break => CfNode::detached(CfInstr::Break(self.rg.loop_label)),
            Expr::Continue => CfNode::detached(CfInstr::Continue(self.rg.loop_label)),
            Expr::Return(it) => {
                let expr = it.as_ref().map(|expr| self.expr_ins(expr));
                CfNode::detached(CfInstr::Return(expr))
            }
            Expr::Decl(interned) => {
                let existing = self.decls.get(interned).copied();
                if let Some(existing) = existing {
                    return self.this_region().nodes.get(existing).clone();
                }

                CfNode::detached(CfInstr::Undef(interned.clone()))
            }
            Expr::Ref(interned) => {
                if let Some(root) = &interned.root {
                    self.expr(root)
                } else {
                    CfNode::detached(CfInstr::Undef(interned.decl.clone()))
                }
            }
            Expr::Ins(ty) => CfNode::detached(CfInstr::Ins(ty.clone())),
            Expr::ContentRef(interned) => todo!(),
            Expr::Pattern(interned) => todo!(),
            Expr::Star => todo!(),
        }
    }

    fn arg(&mut self, arg: &ArgExpr) -> CfArg {
        match arg {
            ArgExpr::Pos(expr) => CfArg::Pos(self.expr_ins(expr)),
            ArgExpr::Named(pair) => {
                let expr = self.expr_ins(&pair.1);
                let name = pair.0.clone();
                CfArg::Named(name, expr)
            }
            ArgExpr::NamedRt(pair) => {
                let name = self.expr_ins(&pair.0);
                let expr = self.expr_ins(&pair.1);
                CfArg::NamedRt(name, expr)
            }
            ArgExpr::Spread(expr) => {
                let expr = self.expr_ins(expr);
                CfArg::Spread(expr)
            }
        }
    }

    fn create_region(&mut self) -> RegionBuilder {
        let region = self.cf.region.create().id;
        std::mem::replace(
            &mut self.rg,
            RegionBuilder {
                region,
                nodes: Vec::new(),
                cont: None,
                loop_label: None,
            },
        )
    }

    fn restore_region(&mut self, rg: RegionBuilder) {
        self.rg = rg;
    }

    fn declare(&mut self, decl: &Interned<Decl>, func: NodeId) {
        self.decls.insert(decl.clone(), func);
    }
}
