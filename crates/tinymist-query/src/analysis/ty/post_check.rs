//! Infer more than the principal type of some expression.

use typst::syntax::{
    ast::{self, AstNode},
    LinkedNode, SyntaxKind,
};

use crate::{syntax::CheckTarget, AnalysisContext};

use super::{FlowType, FlowVarKind, TypeCheckInfo};

// todo: detect recursive usage

/// With given type information, check the type of a literal expression again by
/// touching the possible related nodes.
pub(crate) fn post_type_check(
    _ctx: &mut AnalysisContext,
    info: &TypeCheckInfo,
    node: CheckTarget<'_>,
) -> Option<FlowType> {
    let node = node.node()?;
    let mut worker = PostTypeCheckWorker { _ctx, info };

    worker.check(node)
}

struct PostTypeCheckWorker<'a, 'w> {
    _ctx: &'a mut AnalysisContext<'w>,
    info: &'a TypeCheckInfo,
}

impl<'a, 'w> PostTypeCheckWorker<'a, 'w> {
    fn check(&mut self, node: LinkedNode) -> Option<FlowType> {
        let parent = node.parent()?;
        match parent.kind() {
            SyntaxKind::LetBinding => {
                let p = parent.cast::<ast::LetBinding>()?;
                let exp = p.init()?;
                if exp.span() == node.span() {
                    match p.kind() {
                        ast::LetBindingKind::Closure(_c) => {
                            return None;
                        }
                        ast::LetBindingKind::Normal(pattern) => {
                            return self.destruct_let(pattern, node.clone())
                        }
                    }
                }
            }
            SyntaxKind::Named => {
                let p = parent.cast::<ast::Named>()?;
                let exp = p.expr();
                if exp.span() == node.span() {
                    let ty = self.info.mapping.get(&p.span())?;
                    return self.ubs(ty);
                }
            }
            _ => return None,
        }

        None
    }

    fn destruct_let(&self, pattern: ast::Pattern<'_>, node: LinkedNode<'_>) -> Option<FlowType> {
        match pattern {
            ast::Pattern::Placeholder(_) => None,
            ast::Pattern::Normal(n) => {
                let ast::Expr::Ident(ident) = n else {
                    return None;
                };
                let ty = self.info.mapping.get(&ident.span())?;
                self.ubs(ty)
            }
            ast::Pattern::Parenthesized(p) => {
                self.destruct_let(p.expr().to_untyped().cast()?, node)
            }
            // todo: pattern matching
            ast::Pattern::Destructuring(_d) => {
                let _ = node;
                None
            }
        }
    }

    fn ubs(&self, ty: &FlowType) -> Option<FlowType> {
        match ty {
            FlowType::Let(ty) => Some(FlowType::from_types(ty.ubs.iter().cloned())),
            FlowType::Var(ty) => {
                let v = self.info.vars.get(&ty.0)?;
                match &v.kind {
                    FlowVarKind::Weak(w) => {
                        let r = w.read();
                        Some(FlowType::from_types(r.ubs.iter().cloned()))
                    }
                }
            }
            _ => Some(ty.clone()),
        }
    }
}
