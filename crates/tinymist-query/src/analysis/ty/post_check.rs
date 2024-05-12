//! Infer more than the principal type of some expression.

use std::collections::HashMap;

use hashbrown::HashSet;
use typst::syntax::{
    ast::{self, AstNode},
    LinkedNode, Span, SyntaxKind,
};

use crate::{
    adt::interner::Interned,
    analysis::{ArgsTy, Sig, SigChecker, SigSurfaceKind, TypeBounds},
    syntax::{get_check_target, get_check_target_by_context, CheckTarget, ParamTarget},
    AnalysisContext,
};

use super::{FieldTy, SigShape, Ty, TypeCheckInfo};

/// With given type information, check the type of a literal expression again by
/// touching the possible related nodes.
pub(crate) fn post_type_check(
    _ctx: &mut AnalysisContext,
    info: &TypeCheckInfo,
    node: LinkedNode,
) -> Option<Ty> {
    let mut worker = PostTypeCheckWorker {
        ctx: _ctx,
        checked: HashMap::new(),
        info,
    };

    worker.check(&node)
}

#[derive(Default)]
struct SignatureReceiver {
    lbs_dedup: HashSet<Ty>,
    ubs_dedup: HashSet<Ty>,
    bounds: TypeBounds,
}

impl SignatureReceiver {
    fn insert(&mut self, ty: &Ty, pol: bool) {
        log::debug!("post check receive: {ty:?}");
        if !pol {
            if self.lbs_dedup.insert(ty.clone()) {
                self.bounds.lbs.push(ty.clone());
            }
        } else if self.ubs_dedup.insert(ty.clone()) {
            self.bounds.ubs.push(ty.clone());
        }
    }

    fn finalize(self) -> Ty {
        Ty::Let(self.bounds.into())
    }
}

fn check_signature<'a>(
    receiver: &'a mut SignatureReceiver,
    target: &'a ParamTarget,
) -> impl FnMut(&mut PostTypeCheckWorker, Sig, &[Interned<ArgsTy>], bool) -> Option<()> + 'a {
    move |worker, sig, args, pol| {
        let SigShape { sig: sig_ins, .. } = sig.shape(Some(worker.ctx))?;

        match &target {
            ParamTarget::Named(n) => {
                let ident = n.cast::<ast::Ident>()?;
                let ty = sig_ins.named(&ident.into())?;
                receiver.insert(ty, !pol);

                Some(())
            }
            ParamTarget::Positional {
                // todo: spreads
                spreads: _,
                positional,
                is_spread,
            } => {
                if *is_spread {
                    return None;
                }

                // truncate args
                let c = args
                    .iter()
                    .map(|args| args.positional_params().len())
                    .sum::<usize>();
                let nth = sig_ins.pos(c + positional).or_else(|| sig_ins.rest_param());
                if let Some(nth) = nth {
                    receiver.insert(nth, !pol);
                }

                // names
                for (name, _) in sig_ins.named_params() {
                    // todo: reduce fields, fields ty
                    let field = FieldTy::new_untyped(name.clone());
                    receiver.insert(&Ty::Field(field), !pol);
                }

                Some(())
            }
        }
    }
}

struct PostTypeCheckWorker<'a, 'w> {
    ctx: &'a mut AnalysisContext<'w>,
    checked: HashMap<Span, Option<Ty>>,
    info: &'a TypeCheckInfo,
}

impl<'a, 'w> PostTypeCheckWorker<'a, 'w> {
    fn check(&mut self, node: &LinkedNode) -> Option<Ty> {
        let span = node.span();
        if let Some(ty) = self.checked.get(&span) {
            return ty.clone();
        }
        // loop detection
        self.checked.insert(span, None);

        let ty = self.check_(node);
        self.checked.insert(span, ty.clone());
        ty
    }

    fn check_(&mut self, node: &LinkedNode) -> Option<Ty> {
        let context = node.parent()?;
        log::debug!("post check: {:?}::{:?}", context.kind(), node.kind());
        let checked_context = self.check_context(context, node);
        let res = self.check_self(context, node, checked_context);
        log::debug!(
            "post check(res): {:?}::{:?} -> {res:?}",
            context.kind(),
            node.kind(),
        );
        res
    }

    fn check_context_or(&mut self, context: &LinkedNode, context_ty: Option<Ty>) -> Option<Ty> {
        let checked_context = self.check(context);
        if checked_context.is_some() && context_ty.is_some() {
            let c = checked_context?;
            let s = context_ty?;

            Some(Ty::from_types([c, s].into_iter()))
        } else {
            checked_context.or(context_ty)
        }
    }

    fn check_target(&mut self, node: Option<CheckTarget>, context_ty: Option<Ty>) -> Option<Ty> {
        let Some(node) = node else {
            return context_ty;
        };
        log::debug!("post check target: {node:?}");

        match node {
            CheckTarget::Param {
                callee,
                args: _,
                target,
                is_set,
            } => {
                let callee = self.check_context_or(&callee, context_ty)?;
                log::debug!("post check call target: ({callee:?})::{target:?} is_set: {is_set}");

                let mut resp = SignatureReceiver::default();

                self.check_signatures(&callee, false, &mut check_signature(&mut resp, &target));

                log::debug!("post check target iterated: {:?}", resp.bounds);
                Some(self.info.simplify(resp.finalize(), false))
            }
            CheckTarget::Element { container, target } => {
                let container_ty = self.check_context_or(&container, context_ty)?;
                log::debug!("post check element target: ({container_ty:?})::{target:?}");

                let mut resp = SignatureReceiver::default();

                self.check_element_of(
                    &container_ty,
                    false,
                    &container,
                    &mut check_signature(&mut resp, &target),
                );

                log::debug!("post check target iterated: {:?}", resp.bounds);
                Some(self.info.simplify(resp.finalize(), false))
            }
            CheckTarget::Paren {
                container,
                is_before,
            } => {
                let container_ty = self.check_context_or(&container, context_ty)?;
                log::debug!("post check paren target: {container_ty:?}::{is_before:?}");

                let mut resp = SignatureReceiver::default();
                resp.bounds.lbs.push(container_ty.clone());

                let target = ParamTarget::positional_from_before(true);
                self.check_element_of(
                    &container_ty,
                    false,
                    &container,
                    &mut check_signature(&mut resp, &target),
                );

                log::debug!("post check target iterated: {:?}", resp.bounds);
                Some(self.info.simplify(resp.finalize(), false))
            }
            CheckTarget::Normal(target) => {
                let ty = self.check_context_or(&target, context_ty)?;
                log::debug!("post check target normal: {ty:?}");
                Some(ty)
            }
        }
    }

    fn check_context(&mut self, context: &LinkedNode, node: &LinkedNode) -> Option<Ty> {
        match context.kind() {
            SyntaxKind::LetBinding => {
                let p = context.cast::<ast::LetBinding>()?;
                let exp = p.init()?;
                if exp.span() != node.span() {
                    return None;
                }

                match p.kind() {
                    ast::LetBindingKind::Closure(_c) => None,
                    ast::LetBindingKind::Normal(pattern) => {
                        self.destruct_let(pattern, node.clone())
                    }
                }
            }
            SyntaxKind::Args => self.check_target(
                // todo: not well behaved
                get_check_target_by_context(context.clone(), node.clone()),
                None,
            ),
            // todo: constraint node
            SyntaxKind::Named => self.check_target(get_check_target(context.clone()), None),
            _ => None,
        }
    }

    fn check_self(
        &mut self,
        context: &LinkedNode,
        node: &LinkedNode,
        context_ty: Option<Ty>,
    ) -> Option<Ty> {
        match node.kind() {
            SyntaxKind::Ident => {
                let ty = self.info.type_of_span(node.span());
                log::debug!("post check ident: {node:?} -> {ty:?}");
                self.simplify(&ty?)
            }
            // todo: destructuring
            SyntaxKind::FieldAccess => {
                let ty = self.info.type_of_span(node.span());
                self.simplify(&ty?)
                    .or_else(|| self.check_context_or(context, context_ty))
            }
            _ => self.check_target(get_check_target(node.clone()), context_ty),
        }
    }

    fn destruct_let(&mut self, pattern: ast::Pattern, node: LinkedNode) -> Option<Ty> {
        match pattern {
            ast::Pattern::Placeholder(_) => None,
            ast::Pattern::Normal(n) => {
                let ast::Expr::Ident(ident) = n else {
                    return None;
                };
                self.simplify(&self.info.type_of_span(ident.span())?)
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

    fn check_signatures(
        &mut self,
        ty: &Ty,
        pol: bool,
        checker: &mut impl FnMut(&mut Self, Sig, &[Interned<ArgsTy>], bool) -> Option<()>,
    ) {
        ty.sig_surface(pol, SigSurfaceKind::Call, &mut (self, checker));
    }

    fn check_element_of<T>(&mut self, ty: &Ty, pol: bool, context: &LinkedNode, checker: &mut T)
    where
        T: FnMut(&mut Self, Sig, &[Interned<ArgsTy>], bool) -> Option<()>,
    {
        ty.sig_surface(pol, sig_context_of(context), &mut (self, checker))
    }

    fn simplify(&mut self, ty: &Ty) -> Option<Ty> {
        Some(self.info.simplify(ty.clone(), false))
    }
}

impl<'a, 'w, T> SigChecker for (&mut PostTypeCheckWorker<'a, 'w>, &mut T)
where
    T: FnMut(&mut PostTypeCheckWorker<'a, 'w>, Sig, &[Interned<ArgsTy>], bool) -> Option<()>,
{
    fn check(
        &mut self,
        sig: Sig,
        args: &mut crate::analysis::SigCheckContext,
        pol: bool,
    ) -> Option<()> {
        self.1(self.0, sig, &args.args, pol)
    }

    fn check_var(
        &mut self,
        var: &Interned<crate::analysis::TypeVar>,
        _pol: bool,
    ) -> Option<TypeBounds> {
        self.0
            .info
            .vars
            .get(&var.def)
            .map(|v| v.bounds.bounds().read().clone())
    }
}

fn sig_context_of(context: &LinkedNode) -> SigSurfaceKind {
    match context.kind() {
        SyntaxKind::Parenthesized => SigSurfaceKind::ArrayOrDict,
        SyntaxKind::Array => {
            let c = context.cast::<ast::Array>();
            if c.is_some_and(|e| e.items().next().is_some()) {
                SigSurfaceKind::Array
            } else {
                SigSurfaceKind::ArrayOrDict
            }
        }
        SyntaxKind::Dict => SigSurfaceKind::Dict,
        _ => SigSurfaceKind::Array,
    }
}
