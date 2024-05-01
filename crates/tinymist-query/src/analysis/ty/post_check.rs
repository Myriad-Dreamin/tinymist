//! Infer more than the principal type of some expression.

use std::{collections::HashMap, sync::Arc};

use typst::{
    foundations::Value,
    syntax::{
        ast::{self, AstNode},
        LinkedNode, Span, SyntaxKind,
    },
};

use crate::{
    analysis::{analyze_dyn_signature, FlowVarStore, Signature},
    syntax::{get_check_target, CheckTarget, ParamTarget},
    AnalysisContext,
};

use super::{
    FlowArgs, FlowBuiltinType, FlowRecord, FlowSignature, FlowType, FlowVarKind, TypeCheckInfo,
    FLOW_INSET_DICT, FLOW_MARGIN_DICT, FLOW_OUTSET_DICT, FLOW_RADIUS_DICT, FLOW_STROKE_DICT,
};

/// With given type information, check the type of a literal expression again by
/// touching the possible related nodes.
pub(crate) fn post_type_check(
    _ctx: &mut AnalysisContext,
    info: &TypeCheckInfo,
    node: LinkedNode,
) -> Option<FlowType> {
    let mut worker = PostTypeCheckWorker {
        ctx: _ctx,
        checked: HashMap::new(),
        info,
    };

    worker.check(&node)
}

enum Abstracted<T, V> {
    Type(T),
    Value(V),
}

type AbstractedSignature<'a> = Abstracted<&'a FlowSignature, &'a Signature>;

struct SignatureWrapper<'a>(AbstractedSignature<'a>);

impl<'a> SignatureWrapper<'a> {
    fn named(&self, name: &str) -> Option<&FlowType> {
        match &self.0 {
            Abstracted::Type(sig) => sig.named.iter().find(|(k, _)| k == name).map(|(_, v)| v),
            Abstracted::Value(sig) => sig
                .primary()
                .named
                .get(name)
                .and_then(|p| p.infer_type.as_ref()),
        }
    }

    fn names(&self, mut f: impl FnMut(&str)) {
        match &self.0 {
            Abstracted::Type(sig) => {
                for (k, _) in &sig.named {
                    f(k);
                }
            }
            Abstracted::Value(sig) => {
                for (k, p) in &sig.primary().named {
                    if p.infer_type.is_some() {
                        f(k);
                    }
                }
            }
        }
    }

    fn pos(&self, pos: usize) -> Option<&FlowType> {
        match &self.0 {
            Abstracted::Type(sig) => sig.pos.get(pos),
            // todo: bindings
            Abstracted::Value(sig) => sig
                .primary()
                .pos
                .get(pos)
                .and_then(|p| p.infer_type.as_ref()),
        }
    }

    fn rest(&self) -> Option<&FlowType> {
        match &self.0 {
            Abstracted::Type(sig) => sig.rest.as_ref(),
            Abstracted::Value(sig) => sig
                .primary()
                .rest
                .as_ref()
                .and_then(|p| p.infer_type.as_ref()),
        }
    }
}

#[derive(Default)]
struct SignatureReceiver(FlowVarStore);

impl SignatureReceiver {
    fn insert(&mut self, ty: &FlowType, pol: bool) {
        if pol {
            self.0.lbs.push(ty.clone());
        } else {
            self.0.ubs.push(ty.clone());
        }
    }
}

fn check_signature<'a>(
    receiver: &'a mut SignatureReceiver,
    target: &'a ParamTarget,
) -> impl FnMut(&mut PostTypeCheckWorker, SignatureWrapper, &[FlowArgs], bool) -> Option<()> + 'a {
    move |_worker, sig, args, pol| {
        match &target {
            ParamTarget::Named(n) => {
                let ident = n.cast::<ast::Ident>()?;
                let ty = sig.named(ident.get())?;
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
                let c = args.iter().map(|args| args.args.len()).sum::<usize>();
                let nth = sig.pos(c + positional).or_else(|| sig.rest())?;
                receiver.insert(nth, !pol);

                // names
                sig.names(|name| {
                    // todo: reduce fields
                    receiver.insert(
                        &FlowType::Field(Box::new((name.into(), FlowType::Any, Span::detached()))),
                        !pol,
                    );
                });

                Some(())
            }
        }
    }
}

struct PostTypeCheckWorker<'a, 'w> {
    ctx: &'a mut AnalysisContext<'w>,
    checked: HashMap<Span, Option<FlowType>>,
    info: &'a TypeCheckInfo,
}

impl<'a, 'w> PostTypeCheckWorker<'a, 'w> {
    fn check(&mut self, node: &LinkedNode) -> Option<FlowType> {
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

    fn check_(&mut self, node: &LinkedNode) -> Option<FlowType> {
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

    fn check_context_or(
        &mut self,
        context: &LinkedNode,
        context_ty: Option<FlowType>,
    ) -> Option<FlowType> {
        let checked_context = self.check(context);
        if checked_context.is_some() && context_ty.is_some() {
            let c = checked_context?;
            let s = context_ty?;

            Some(FlowType::from_types([c, s].into_iter()))
        } else {
            checked_context.or(context_ty)
        }
    }

    fn check_target(
        &mut self,
        node: Option<CheckTarget>,
        context_ty: Option<FlowType>,
    ) -> Option<FlowType> {
        let Some(node) = node else {
            return context_ty;
        };
        log::debug!("post check target: {node:?}");

        match node {
            CheckTarget::Param {
                callee,
                target,
                is_set,
            } => {
                let callee = self.check_context_or(&callee, context_ty)?;
                log::debug!("post check call target: ({callee:?})::{target:?} is_set: {is_set}");

                let mut resp = SignatureReceiver::default();

                self.check_signatures(&callee, false, &mut check_signature(&mut resp, &target));

                log::debug!("post check target iterated: {:?}", resp.0);
                Some(self.info.simplify(FlowType::Let(Arc::new(resp.0)), false))
            }
            CheckTarget::Element { container, target } => {
                let container_ty = self.check_context_or(&container, context_ty)?;
                log::debug!("post check element target: {container_ty:?}::{target:?}");

                let mut resp = SignatureReceiver::default();

                self.check_element_of(
                    &container_ty,
                    false,
                    &container,
                    &mut check_signature(&mut resp, &target),
                );

                log::debug!("post check target iterated: {:?}", resp.0);
                Some(self.info.simplify(FlowType::Let(Arc::new(resp.0)), false))
            }
            CheckTarget::Paren {
                container,
                is_before,
            } => {
                let container_ty = self.check_context_or(&container, context_ty)?;
                log::info!("post check param target: {container_ty:?}::{is_before:?}");

                let mut resp = SignatureReceiver::default();
                resp.0.lbs.push(container_ty.clone());

                let target = ParamTarget::positional_from_before(true);
                self.check_element_of(
                    &container_ty,
                    false,
                    &container,
                    &mut check_signature(&mut resp, &target),
                );

                log::debug!("post check target iterated: {:?}", resp.0);
                Some(self.info.simplify(FlowType::Let(Arc::new(resp.0)), false))
            }
            CheckTarget::Normal(target) => {
                let ty = self.check_context_or(&target, context_ty)?;
                log::debug!("post check target: {ty:?}");
                Some(ty)
            }
        }
    }

    fn check_context(&mut self, context: &LinkedNode, node: &LinkedNode) -> Option<FlowType> {
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
            // todo: constraint node
            SyntaxKind::Args | SyntaxKind::Named => {
                self.check_target(get_check_target(context.clone()), None)
            }
            _ => None,
        }
    }

    fn check_self(
        &mut self,
        context: &LinkedNode,
        node: &LinkedNode,
        context_ty: Option<FlowType>,
    ) -> Option<FlowType> {
        match node.kind() {
            SyntaxKind::Ident => {
                let ident = node.cast::<ast::Ident>()?;
                let ty = self.info.mapping.get(&ident.span());
                log::debug!("post check ident: {ident:?} -> {ty:?}");
                self.simplify(ty?)
            }
            // todo: destructuring
            SyntaxKind::FieldAccess => self.check_context_or(context, context_ty),
            _ => self.check_target(get_check_target(node.clone()), context_ty),
        }
    }

    fn destruct_let(&mut self, pattern: ast::Pattern, node: LinkedNode) -> Option<FlowType> {
        match pattern {
            ast::Pattern::Placeholder(_) => None,
            ast::Pattern::Normal(n) => {
                let ast::Expr::Ident(ident) = n else {
                    return None;
                };
                let ty = self.info.mapping.get(&ident.span())?;
                self.simplify(ty)
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
        ty: &FlowType,
        pol: bool,
        checker: &mut impl FnMut(&mut Self, SignatureWrapper, &[FlowArgs], bool) -> Option<()>,
    ) {
        self.check_signatures_(ty, pol, SigParamKind::Call, &mut Vec::new(), checker);
    }

    fn check_element_of(
        &mut self,
        ty: &FlowType,
        pol: bool,
        context: &LinkedNode,
        checker: &mut impl FnMut(&mut Self, SignatureWrapper, &[FlowArgs], bool) -> Option<()>,
    ) {
        self.check_signatures_(ty, pol, sig_context_of(context), &mut Vec::new(), checker);
    }

    fn check_signatures_(
        &mut self,
        ty: &FlowType,
        pol: bool,
        sig_kind: SigParamKind,
        args: &mut Vec<FlowArgs>,
        checker: &mut impl FnMut(&mut Self, SignatureWrapper, &[FlowArgs], bool) -> Option<()>,
    ) {
        match ty {
            FlowType::Builtin(FlowBuiltinType::Stroke)
                if matches!(sig_kind, SigParamKind::Dict | SigParamKind::ArrayOrDict) =>
            {
                self.check_dict_signature(&FLOW_STROKE_DICT, pol, checker);
            }
            FlowType::Builtin(FlowBuiltinType::Margin)
                if matches!(sig_kind, SigParamKind::Dict | SigParamKind::ArrayOrDict) =>
            {
                self.check_dict_signature(&FLOW_MARGIN_DICT, pol, checker);
            }
            FlowType::Builtin(FlowBuiltinType::Inset)
                if matches!(sig_kind, SigParamKind::Dict | SigParamKind::ArrayOrDict) =>
            {
                self.check_dict_signature(&FLOW_INSET_DICT, pol, checker);
            }
            FlowType::Builtin(FlowBuiltinType::Outset)
                if matches!(sig_kind, SigParamKind::Dict | SigParamKind::ArrayOrDict) =>
            {
                self.check_dict_signature(&FLOW_OUTSET_DICT, pol, checker);
            }
            FlowType::Builtin(FlowBuiltinType::Radius)
                if matches!(sig_kind, SigParamKind::Dict | SigParamKind::ArrayOrDict) =>
            {
                self.check_dict_signature(&FLOW_RADIUS_DICT, pol, checker);
            }
            FlowType::Func(sig) if sig_kind == SigParamKind::Call => {
                checker(self, SignatureWrapper(Abstracted::Type(sig)), args, pol);
            }
            FlowType::Array(sig)
                if matches!(sig_kind, SigParamKind::Array | SigParamKind::ArrayOrDict) =>
            {
                let sig = FlowSignature::array_cons(*sig.clone(), true);
                checker(self, SignatureWrapper(Abstracted::Type(&sig)), args, pol);
            }
            FlowType::Dict(sig)
                if matches!(sig_kind, SigParamKind::Dict | SigParamKind::ArrayOrDict) =>
            {
                self.check_dict_signature(sig, pol, checker);
            }
            FlowType::With(w) if sig_kind == SigParamKind::Call => {
                let c = args.len();
                args.extend(w.1.iter().cloned());
                self.check_signatures_(&w.0, pol, sig_kind, args, checker);
                args.truncate(c);
            }
            FlowType::Union(u) => {
                for ty in u.iter() {
                    self.check_signatures_(ty, pol, sig_kind, args, checker);
                }
            }
            FlowType::Let(u) => {
                for lb in &u.ubs {
                    self.check_signatures_(lb, pol, sig_kind, args, checker);
                }
                for ub in &u.lbs {
                    self.check_signatures_(ub, !pol, sig_kind, args, checker);
                }
            }
            FlowType::Var(u) => {
                let Some(v) = self.info.vars.get(&u.0) else {
                    return;
                };
                match &v.kind {
                    FlowVarKind::Weak(w) => {
                        let r = w.read();
                        for lb in &r.ubs {
                            self.check_signatures_(lb, pol, sig_kind, args, checker);
                        }
                        for ub in &r.lbs {
                            self.check_signatures_(ub, !pol, sig_kind, args, checker);
                        }
                    }
                }
            }
            // todo: deduplicate checking early
            FlowType::Value(v) => {
                if sig_kind == SigParamKind::Call {
                    if let Value::Func(f) = &v.0 {
                        let sig = analyze_dyn_signature(self.ctx, f.clone());
                        checker(self, SignatureWrapper(Abstracted::Value(&sig)), args, pol);
                    }
                }
            }
            FlowType::ValueDoc(v) => {
                if sig_kind == SigParamKind::Call {
                    if let Value::Func(f) = &v.0 {
                        let sig = analyze_dyn_signature(self.ctx, f.clone());
                        checker(self, SignatureWrapper(Abstracted::Value(&sig)), args, pol);
                    }
                }
            }
            _ => {}
        }
    }

    fn check_dict_signature(
        &mut self,
        sig: &FlowRecord,
        pol: bool,
        checker: &mut impl FnMut(&mut Self, SignatureWrapper, &[FlowArgs], bool) -> Option<()>,
    ) {
        let sig = FlowSignature::dict_cons(sig, true);
        checker(self, SignatureWrapper(Abstracted::Type(&sig)), &[], pol);
    }

    fn simplify(&mut self, ty: &FlowType) -> Option<FlowType> {
        Some(self.info.simplify(ty.clone(), false))
    }
}

fn sig_context_of(context: &LinkedNode) -> SigParamKind {
    match context.kind() {
        SyntaxKind::Parenthesized => SigParamKind::ArrayOrDict,
        SyntaxKind::Array => {
            let c = context.cast::<ast::Array>();
            if c.is_some_and(|e| e.items().next().is_some()) {
                SigParamKind::ArrayOrDict
            } else {
                SigParamKind::Array
            }
        }
        SyntaxKind::Dict => SigParamKind::Dict,
        _ => SigParamKind::Array,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SigParamKind {
    Call,
    Array,
    Dict,
    ArrayOrDict,
}
