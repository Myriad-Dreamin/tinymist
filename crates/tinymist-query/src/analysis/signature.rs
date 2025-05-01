//! Analysis of function signatures.

use itertools::Either;
use tinymist_analysis::{func_signature, ArgInfo, ArgsInfo, PartialSignature};
use tinymist_derive::BindTyCtx;

use super::{prelude::*, Definition, SharedContext};
use crate::analysis::PostTypeChecker;
use crate::docs::{UntypedDefDocs, UntypedSignatureDocs, UntypedVarDocs};
use crate::syntax::classify_def_loosely;
use crate::ty::{
    BoundChecker, DocSource, DynTypeBounds, ParamAttrs, ParamTy, SigWithTy, TyCtx, TypeInfo,
    TypeVar,
};

pub use tinymist_analysis::{PrimarySignature, Signature};

/// The language object that the signature is being analyzed for.
#[derive(Debug, Clone)]
pub enum SignatureTarget {
    /// A static node without knowing the function at runtime.
    Def(Option<Source>, Definition),
    /// A static node without knowing the function at runtime.
    SyntaxFast(Source, Span),
    /// A static node without knowing the function at runtime.
    Syntax(Source, Span),
    /// A function that is known at runtime.
    Runtime(Func),
    /// A function that is known at runtime.
    Convert(Func),
}

pub(crate) fn analyze_signature(
    ctx: &Arc<SharedContext>,
    callee_node: SignatureTarget,
) -> Option<Signature> {
    ctx.compute_signature(callee_node.clone(), move |ctx| {
        crate::log_debug_ct!("analyzing signature for {callee_node:?}");
        analyze_type_signature(ctx, &callee_node)
            .or_else(|| analyze_dyn_signature(ctx, &callee_node))
    })
}

fn analyze_type_signature(
    ctx: &Arc<SharedContext>,
    callee_node: &SignatureTarget,
) -> Option<Signature> {
    let (type_info, ty) = match callee_node {
        SignatureTarget::Convert(..) => return None,
        SignatureTarget::SyntaxFast(source, span) | SignatureTarget::Syntax(source, span) => {
            let type_info = ctx.type_check(source);
            let ty = type_info.type_of_span(*span)?;
            Some((type_info, ty))
        }
        SignatureTarget::Def(source, def) => {
            let span = def.decl.span();
            let type_info = ctx.type_check(source.as_ref()?);
            let ty = type_info.type_of_span(span)?;
            Some((type_info, ty))
        }
        SignatureTarget::Runtime(func) => {
            let source = ctx.source_by_id(func.span().id()?).ok()?;
            let node = source.find(func.span())?;
            let def = classify_def_loosely(node.parent()?.clone())?;
            let type_info = ctx.type_check(&source);
            let ty = type_info.type_of_span(def.name()?.span())?;
            Some((type_info, ty))
        }
    }?;

    sig_of_type(ctx, &type_info, ty)
}

pub(crate) fn sig_of_type(
    ctx: &Arc<SharedContext>,
    type_info: &TypeInfo,
    ty: Ty,
) -> Option<Signature> {
    // todo multiple sources
    let mut srcs = ty.sources();
    srcs.sort();
    crate::log_debug_ct!("check type signature of ty: {ty:?} => {srcs:?}");
    let type_var = srcs.into_iter().next()?;
    match type_var {
        DocSource::Var(v) => {
            let mut ty_ctx = PostTypeChecker::new(ctx.clone(), type_info);
            let sig_ty = Ty::Func(ty.sig_repr(true, &mut ty_ctx)?);
            let sig_ty = type_info.simplify(sig_ty, false);
            let Ty::Func(sig_ty) = sig_ty else {
                static WARN_ONCE: std::sync::Once = std::sync::Once::new();
                WARN_ONCE.call_once(|| {
                    // todo: seems like a bug
                    log::warn!("expected function type, got {sig_ty:?}");
                });
                return None;
            };

            // todo: this will affect inlay hint: _var_with
            let (var_with, docstring) = match type_info.var_docs.get(&v.def).map(|x| x.as_ref()) {
                Some(UntypedDefDocs::Function(sig)) => (vec![], Either::Left(sig.as_ref())),
                Some(UntypedDefDocs::Variable(docs)) => find_alias_stack(&mut ty_ctx, &v, docs)?,
                _ => return None,
            };

            let docstring = match docstring {
                Either::Left(docstring) => docstring,
                Either::Right(func) => return Some(wind_stack(var_with, ctx.type_of_func(func))),
            };

            let mut param_specs = Vec::new();
            let mut has_fill_or_size_or_stroke = false;
            let mut _broken = false;

            if docstring.pos.len() != sig_ty.positional_params().len() {
                static WARN_ONCE: std::sync::Once = std::sync::Once::new();
                WARN_ONCE.call_once(|| {
                    // todo: seems like a bug
                    log::warn!("positional params mismatch: {docstring:#?} != {sig_ty:#?}");
                });
                return None;
            }

            for (doc, ty) in docstring.pos.iter().zip(sig_ty.positional_params()) {
                let default = doc.default.clone();
                let ty = ty.clone();

                let name = doc.name.clone();
                if matches!(name.as_ref(), "fill" | "stroke" | "size") {
                    has_fill_or_size_or_stroke = true;
                }

                param_specs.push(Interned::new(ParamTy {
                    name,
                    docs: Some(doc.docs.clone()),
                    default,
                    ty,
                    attrs: ParamAttrs::positional(),
                }));
            }

            for (name, ty) in sig_ty.named_params() {
                let docstring = docstring.named.get(name);
                let default = Some(
                    docstring
                        .and_then(|doc| doc.default.clone())
                        .unwrap_or_else(|| "unknown".into()),
                );
                let ty = ty.clone();

                if matches!(name.as_ref(), "fill" | "stroke" | "size") {
                    has_fill_or_size_or_stroke = true;
                }

                param_specs.push(Interned::new(ParamTy {
                    name: name.clone(),
                    docs: docstring.map(|doc| doc.docs.clone()),
                    default,
                    ty,
                    attrs: ParamAttrs::named(),
                }));
            }

            if let Some(doc) = docstring.rest.as_ref() {
                let default = doc.default.clone();

                param_specs.push(Interned::new(ParamTy {
                    name: doc.name.clone(),
                    docs: Some(doc.docs.clone()),
                    default,
                    ty: sig_ty.rest_param().cloned().unwrap_or(Ty::Any),
                    attrs: ParamAttrs::variadic(),
                }));
            }

            let sig = Signature::Primary(Arc::new(PrimarySignature {
                docs: Some(docstring.docs.clone()),
                param_specs,
                has_fill_or_size_or_stroke,
                sig_ty,
                _broken,
            }));
            Some(wind_stack(var_with, sig))
        }
        src @ (DocSource::Builtin(..) | DocSource::Ins(..)) => {
            Some(ctx.type_of_func(src.as_func()?))
        }
    }
}

fn wind_stack(var_with: Vec<WithElem>, sig: Signature) -> Signature {
    if var_with.is_empty() {
        return sig;
    }

    let (primary, mut base_args) = match sig {
        Signature::Primary(primary) => (primary, eco_vec![]),
        Signature::Partial(partial) => (partial.signature.clone(), partial.with_stack.clone()),
    };

    let mut accepting = primary.pos().iter().skip(base_args.len());

    // Ignoring docs at the moment
    for (_d, w) in var_with {
        if let Some(w) = w {
            let mut items = eco_vec![];
            for pos in w.with.positional_params() {
                let Some(arg) = accepting.next() else {
                    break;
                };
                items.push(ArgInfo {
                    name: Some(arg.name.clone()),
                    term: Some(pos.clone()),
                });
            }
            // todo: ignored spread arguments
            if !items.is_empty() {
                base_args.push(ArgsInfo { items });
            }
        }
    }

    Signature::Partial(Arc::new(PartialSignature {
        signature: primary,
        with_stack: base_args,
    }))
}

type WithElem<'a> = (&'a UntypedVarDocs, Option<Interned<SigWithTy>>);

fn find_alias_stack<'a>(
    ctx: &'a mut PostTypeChecker,
    var: &Interned<TypeVar>,
    docs: &'a UntypedVarDocs,
) -> Option<(Vec<WithElem<'a>>, Either<&'a UntypedSignatureDocs, Func>)> {
    let mut checker = AliasStackChecker {
        ctx,
        stack: vec![(docs, None)],
        res: None,
        checking_with: true,
    };
    Ty::Var(var.clone()).bounds(true, &mut checker);

    checker.res.map(|res| (checker.stack, res))
}

#[derive(BindTyCtx)]
#[bind(ctx)]
struct AliasStackChecker<'a, 'b> {
    ctx: &'a mut PostTypeChecker<'b>,
    stack: Vec<WithElem<'a>>,
    res: Option<Either<&'a UntypedSignatureDocs, Func>>,
    checking_with: bool,
}

impl BoundChecker for AliasStackChecker<'_, '_> {
    fn check_var(&mut self, u: &Interned<TypeVar>, pol: bool) {
        crate::log_debug_ct!("collecting var {u:?} {pol:?}");
        if self.res.is_some() {
            return;
        }

        if self.checking_with {
            self.check_var_rec(u, pol);
            return;
        }

        let docs = self.ctx.info.var_docs.get(&u.def).map(|x| x.as_ref());

        crate::log_debug_ct!("collecting var {u:?} {pol:?} => {docs:?}");
        // todo: bind builtin functions
        match docs {
            Some(UntypedDefDocs::Function(sig)) => {
                self.res = Some(Either::Left(sig));
            }
            Some(UntypedDefDocs::Variable(docs)) => {
                self.checking_with = true;
                self.stack.push((docs, None));
                self.check_var_rec(u, pol);
                self.stack.pop();
                self.checking_with = false;
            }
            _ => {}
        }
    }

    fn collect(&mut self, ty: &Ty, pol: bool) {
        if self.res.is_some() {
            return;
        }

        match (self.checking_with, ty) {
            (true, Ty::With(w)) => {
                crate::log_debug_ct!("collecting with {ty:?} {pol:?}");
                self.stack.last_mut().unwrap().1 = Some(w.clone());
                self.checking_with = false;
                w.sig.bounds(pol, self);
                self.checking_with = true;
            }
            (false, ty) => {
                if let Some(src) = ty.as_source() {
                    match src {
                        DocSource::Var(u) => {
                            self.check_var(&u, pol);
                        }
                        src @ (DocSource::Builtin(..) | DocSource::Ins(..)) => {
                            if let Some(func) = src.as_func() {
                                self.res = Some(Either::Right(func));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn analyze_dyn_signature(
    ctx: &Arc<SharedContext>,
    callee_node: &SignatureTarget,
) -> Option<Signature> {
    let func = match callee_node {
        SignatureTarget::Def(_source, def) => def.value()?.to_func()?,
        SignatureTarget::SyntaxFast(..) => return None,
        SignatureTarget::Syntax(source, span) => {
            let def = ctx.def_of_span(source, None, *span)?;
            def.value()?.to_func()?
        }
        SignatureTarget::Convert(func) | SignatureTarget::Runtime(func) => func.clone(),
    };

    Some(func_signature(func))
}
