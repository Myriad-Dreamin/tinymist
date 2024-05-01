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

use super::{FlowArgs, FlowSignature, FlowType, FlowVarKind, TypeCheckInfo};

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
}

// todo: detect recursive usage

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
        let checked_self = self.check_self(context, node);

        if checked_context.is_some() && checked_self.is_some() {
            let c = checked_context?;
            let s = checked_self?;

            Some(FlowType::from_types([c, s].into_iter()))
        } else {
            checked_context.or(checked_self)
        }
    }

    fn check_target(&mut self, node: CheckTarget) -> Option<FlowType> {
        log::debug!("post check target: {node:?}");

        match node {
            CheckTarget::Param {
                callee,
                target,
                is_set,
            } => {
                let callee = self.check(&callee)?;
                log::debug!("post check target: ({callee:?})::{target:?} is_set: {is_set}");

                let mut resp = FlowVarStore::default();
                let mut insert_pol = |ty: &FlowType, pol: bool| {
                    if pol {
                        resp.lbs.push(ty.clone());
                    } else {
                        resp.ubs.push(ty.clone());
                    }
                };

                self.check_signatures(
                    &callee,
                    false,
                    &mut |_worker, sig, args, pol| match &target {
                        ParamTarget::Named(n) => {
                            let ident = n.cast::<ast::Ident>()?;
                            let ty = sig.named(ident.get())?;
                            insert_pol(ty, !pol);

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
                            let nth = sig.pos(c + positional)?;
                            insert_pol(nth, !pol);

                            Some(())
                        }
                    },
                );

                log::debug!("post check target iterated: {resp:?}");
                Some(self.info.simplify(FlowType::Let(Arc::new(resp)), false))
            }
            CheckTarget::Normal(target) => {
                let ty = self.check(&target)?;
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
            SyntaxKind::Named => self.check_target(get_check_target(context.clone())?),
            _ => None,
        }
    }

    fn check_self(&mut self, context: &LinkedNode, node: &LinkedNode) -> Option<FlowType> {
        match node.kind() {
            SyntaxKind::LeftParen => match context.kind() {
                SyntaxKind::FuncCall => self.check_target(get_check_target(node.clone())?),
                // todo: constrain as checker
                // constraint(check(context), node)
                _ => self.check(context),
            },
            SyntaxKind::RightParen => match context.kind() {
                SyntaxKind::FuncCall => self.check_target(get_check_target(node.clone())?),
                SyntaxKind::Array | SyntaxKind::Dict => self.element_of(context),
                _ => self.check(context),
            },
            SyntaxKind::Ident => {
                let ident = node.cast::<ast::Ident>()?;
                let ty = self.info.mapping.get(&ident.span());
                log::debug!("post check ident: {ident:?} -> {ty:?}");
                self.simplify(ty?)
            }
            // SyntaxKind::Args => self.check_target(get_check_target(node.clone())?),
            _ => None,
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
        self.check_signatures_(ty, pol, &mut Vec::new(), checker);
    }

    fn check_signatures_(
        &mut self,
        ty: &FlowType,
        pol: bool,
        args: &mut Vec<FlowArgs>,
        checker: &mut impl FnMut(&mut Self, SignatureWrapper, &[FlowArgs], bool) -> Option<()>,
    ) {
        match ty {
            FlowType::Func(sig) => {
                checker(self, SignatureWrapper(Abstracted::Type(sig)), args, pol);
            }
            FlowType::With(w) => {
                let c = args.len();
                args.extend(w.1.iter().cloned());
                self.check_signatures_(&w.0, pol, args, checker);
                args.truncate(c);
            }
            FlowType::Union(u) => {
                for ty in u.iter() {
                    self.check_signatures_(ty, pol, args, checker);
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
                            self.check_signatures_(lb, pol, args, checker);
                        }
                        for ub in &r.lbs {
                            self.check_signatures_(ub, !pol, args, checker);
                        }
                    }
                }
            }
            // todo: deduplicate checking early
            FlowType::Value(v) => {
                if let Value::Func(f) = &v.0 {
                    let sig = analyze_dyn_signature(self.ctx, f.clone());
                    checker(self, SignatureWrapper(Abstracted::Value(&sig)), args, pol);
                }
            }
            FlowType::ValueDoc(v) => {
                if let Value::Func(f) = &v.0 {
                    let sig = analyze_dyn_signature(self.ctx, f.clone());
                    checker(self, SignatureWrapper(Abstracted::Value(&sig)), args, pol);
                }
            }
            _ => {}
        }
    }

    fn simplify(&mut self, ty: &FlowType) -> Option<FlowType> {
        Some(self.info.simplify(ty.clone(), false))
    }

    fn element_of(&mut self, context: &LinkedNode) -> Option<FlowType> {
        let ty = self.check(context)?;
        log::debug!("post check element_of: {ty:?}");
        None
    }
}
