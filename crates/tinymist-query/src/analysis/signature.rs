//! Analysis of function signatures.

use itertools::Either;
use tinymist_derive::BindTyCtx;
use typst::foundations::Closure;

use super::{
    prelude::*, BoundChecker, Definition, DocSource, ParamTy, SharedContext, SigTy, SigWithTy,
    TypeScheme, TypeVar,
};
use crate::analysis::PostTypeChecker;
use crate::docs::{UntypedDefDocs, UntypedSignatureDocs, UntypedVarDocs};
use crate::syntax::get_non_strict_def_target;
use crate::ty::{DynTypeBounds, ParamAttrs};
use crate::ty::{InsTy, TyCtx};
use crate::upstream::truncated_repr;

/// Describes a function signature.
#[derive(Debug, Clone)]
pub enum Signature {
    /// A primary function signature.
    Primary(Arc<PrimarySignature>),
    /// A partially applied function signature.
    Partial(Arc<PartialSignature>),
}

impl Signature {
    /// Returns the primary signature if it is one.
    pub fn primary(&self) -> &Arc<PrimarySignature> {
        match self {
            Signature::Primary(sig) => sig,
            Signature::Partial(sig) => &sig.signature,
        }
    }

    /// Returns the with bindings of the signature.
    pub fn bindings(&self) -> &[ArgsInfo] {
        match self {
            Signature::Primary(_) => &[],
            Signature::Partial(sig) => &sig.with_stack,
        }
    }

    /// Returns the all parameters of the function.
    pub(crate) fn params(&self) -> impl Iterator<Item = (&Interned<ParamTy>, Option<&Ty>)> {
        let primary = self.primary().params();
        // todo: with stack
        primary
    }

    pub(crate) fn type_sig(&self) -> Interned<SigTy> {
        let primary = self.primary().sig_ty.clone();
        // todo: with stack
        primary
    }

    pub(crate) fn param_shift(&self) -> usize {
        match self {
            Signature::Primary(_) => 0,
            Signature::Partial(sig) => sig
                .with_stack
                .iter()
                .map(|ws| ws.items.len())
                .sum::<usize>(),
        }
    }
}

/// Describes a primary function signature.
#[derive(Debug, Clone)]
pub struct PrimarySignature {
    /// The documentation of the function
    pub docs: Option<EcoString>,
    /// The documentation of the parameter.
    pub param_specs: Vec<Interned<ParamTy>>,
    /// Whether the function has fill, stroke, or size parameters.
    pub has_fill_or_size_or_stroke: bool,
    /// The associated signature type.
    pub(crate) sig_ty: Interned<SigTy>,
    _broken: bool,
}

impl PrimarySignature {
    /// Returns the number of positional parameters of the function.
    pub fn pos_size(&self) -> usize {
        self.sig_ty.name_started as usize
    }

    /// Returns the positional parameters of the function.
    pub fn pos(&self) -> &[Interned<ParamTy>] {
        &self.param_specs[..self.pos_size()]
    }

    /// Returns the positional parameters of the function.
    pub fn get_pos(&self, offset: usize) -> Option<&Interned<ParamTy>> {
        self.pos().get(offset)
    }

    /// Returns the named parameters of the function.
    pub fn named(&self) -> &[Interned<ParamTy>] {
        &self.param_specs[self.pos_size()..self.pos_size() + self.sig_ty.names.names.len()]
    }

    /// Returns the named parameters of the function.
    pub fn get_named(&self, name: &StrRef) -> Option<&Interned<ParamTy>> {
        self.named().get(self.sig_ty.names.find(name)?)
    }

    /// Returns the name of the rest parameter of the function.
    pub fn has_spread_right(&self) -> bool {
        self.sig_ty.spread_right
    }

    /// Returns the rest parameter of the function.
    pub fn rest(&self) -> Option<&Interned<ParamTy>> {
        self.has_spread_right()
            .then(|| &self.param_specs[self.pos_size() + self.sig_ty.names.names.len()])
    }

    /// Returns the all parameters of the function.
    pub fn params(&self) -> impl Iterator<Item = (&Interned<ParamTy>, Option<&Ty>)> {
        let pos = self.pos();
        let named = self.named();
        let rest = self.rest();
        let type_sig = &self.sig_ty;
        let pos = pos
            .iter()
            .enumerate()
            .map(|(i, pos)| (pos, type_sig.pos(i)));
        let named = named.iter().map(|x| (x, type_sig.named(&x.name)));
        let rest = rest.into_iter().map(|x| (x, type_sig.rest_param()));

        pos.chain(named).chain(rest)
    }
}

/// Describes a function argument instance
#[derive(Debug, Clone)]
pub struct ArgInfo {
    /// The argument's name.
    pub name: Option<StrRef>,
    /// The argument's term.
    pub term: Option<Ty>,
}

/// Describes a function argument list.
#[derive(Debug, Clone)]
pub struct ArgsInfo {
    /// The arguments.
    pub items: EcoVec<ArgInfo>,
}

/// Describes a function signature that is already partially applied.
#[derive(Debug, Clone)]
pub struct PartialSignature {
    /// The positional parameters.
    pub signature: Arc<PrimarySignature>,
    /// The stack of `fn.with(..)` calls.
    pub with_stack: EcoVec<ArgsInfo>,
}

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
        SignatureTarget::Runtime(f) => {
            let source = ctx.source_by_id(f.span().id()?).ok()?;
            let node = source.find(f.span())?;
            let def = get_non_strict_def_target(node.parent()?.clone())?;
            let type_info = ctx.type_check(&source);
            let ty = type_info.type_of_span(def.name()?.span())?;
            Some((type_info, ty))
        }
    }?;

    sig_of_type(ctx, &type_info, ty)
}

pub(crate) fn sig_of_type(
    ctx: &Arc<SharedContext>,
    type_info: &TypeScheme,
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
                Some(UntypedDefDocs::Variable(d)) => find_alias_stack(&mut ty_ctx, &v, d)?,
                _ => return None,
            };

            let docstring = match docstring {
                Either::Left(docstring) => docstring,
                Either::Right(f) => return Some(wind_stack(var_with, ctx.type_of_func(f))),
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
    v: &Interned<TypeVar>,
    d: &'a UntypedVarDocs,
) -> Option<(Vec<WithElem<'a>>, Either<&'a UntypedSignatureDocs, Func>)> {
    let mut checker = AliasStackChecker {
        ctx,
        stack: vec![(d, None)],
        res: None,
        checking_with: true,
    };
    Ty::Var(v.clone()).bounds(true, &mut checker);

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
            Some(UntypedDefDocs::Variable(d)) => {
                self.checking_with = true;
                self.stack.push((d, None));
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
                            if let Some(f) = src.as_func() {
                                self.res = Some(Either::Right(f));
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

/// Gets the signature of a function.
#[comemo::memoize]
pub fn func_signature(func: Func) -> Signature {
    use typst::foundations::func::Repr;
    let mut with_stack = eco_vec![];
    let mut func = func;
    while let Repr::With(f) = func.inner() {
        with_stack.push(ArgsInfo {
            items: f
                .1
                .items
                .iter()
                .map(|arg| ArgInfo {
                    name: arg.name.clone().map(From::from),
                    term: Some(Ty::Value(InsTy::new(arg.value.v.clone()))),
                })
                .collect(),
        });
        func = f.0.clone();
    }

    let mut pos_tys = vec![];
    let mut named_tys = Vec::new();
    let mut rest_ty = None;

    let mut named_specs = BTreeMap::new();
    let mut param_specs = Vec::new();
    let mut rest_spec = None;

    let mut broken = false;
    let mut has_fill_or_size_or_stroke = false;

    let mut add_param = |param: Interned<ParamTy>| {
        let name = param.name.clone();
        if param.attrs.named {
            if matches!(name.as_ref(), "fill" | "stroke" | "size") {
                has_fill_or_size_or_stroke = true;
            }
            named_tys.push((name.clone(), param.ty.clone()));
            named_specs.insert(name.clone(), param.clone());
        }

        if param.attrs.variadic {
            if rest_ty.is_some() {
                broken = true;
            } else {
                rest_ty = Some(param.ty.clone());
                rest_spec = Some(param);
            }
        } else if param.attrs.positional {
            // todo: we have some params that are both positional and named
            pos_tys.push(param.ty.clone());
            param_specs.push(param);
        }
    };

    let ret_ty = match func.inner() {
        Repr::With(..) => unreachable!(),
        Repr::Closure(c) => {
            analyze_closure_signature(c.clone(), &mut add_param);
            None
        }
        Repr::Element(..) | Repr::Native(..) => {
            for p in func.params().unwrap() {
                add_param(Interned::new(ParamTy {
                    name: p.name.into(),
                    docs: Some(p.docs.into()),
                    default: p.default.map(|d| truncated_repr(&d())),
                    ty: Ty::from_param_site(&func, p),
                    attrs: p.into(),
                }));
            }

            func.returns().map(|r| Ty::from_return_site(&func, r))
        }
    };

    let sig_ty = SigTy::new(pos_tys.into_iter(), named_tys, None, rest_ty, ret_ty);

    for name in &sig_ty.names.names {
        param_specs.push(named_specs.get(name).unwrap().clone());
    }
    if let Some(doc) = rest_spec {
        param_specs.push(doc);
    }

    let signature = Arc::new(PrimarySignature {
        docs: func.docs().map(From::from),
        param_specs,
        has_fill_or_size_or_stroke,
        sig_ty: sig_ty.into(),
        _broken: broken,
    });

    log::trace!("got signature {signature:?}");

    if with_stack.is_empty() {
        return Signature::Primary(signature);
    }

    Signature::Partial(Arc::new(PartialSignature {
        signature,
        with_stack,
    }))
}

fn analyze_closure_signature(
    c: Arc<LazyHash<Closure>>,
    add_param: &mut impl FnMut(Interned<ParamTy>),
) {
    log::trace!("closure signature for: {:?}", c.node.kind());

    let closure = &c.node;
    let closure_ast = match closure.kind() {
        SyntaxKind::Closure => closure.cast::<ast::Closure>().unwrap(),
        _ => return,
    };

    for param in closure_ast.params().children() {
        match param {
            ast::Param::Pos(e) => {
                let name = format!("{}", PatternDisplay(&e));
                add_param(Interned::new(ParamTy {
                    name: name.as_str().into(),
                    docs: None,
                    default: None,
                    ty: Ty::Any,
                    attrs: ParamAttrs::positional(),
                }));
            }
            // todo: pattern
            ast::Param::Named(n) => {
                let expr = unwrap_expr(n.expr()).to_untyped().clone().into_text();
                add_param(Interned::new(ParamTy {
                    name: n.name().get().into(),
                    docs: Some(eco_format!("Default value: {expr}")),
                    default: Some(expr),
                    ty: Ty::Any,
                    attrs: ParamAttrs::named(),
                }));
            }
            ast::Param::Spread(n) => {
                let ident = n.sink_ident().map(|e| e.as_str());
                add_param(Interned::new(ParamTy {
                    name: ident.unwrap_or_default().into(),
                    docs: None,
                    default: None,
                    ty: Ty::Any,
                    attrs: ParamAttrs::variadic(),
                }));
            }
        }
    }
}

struct PatternDisplay<'a>(&'a ast::Pattern<'a>);

impl fmt::Display for PatternDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ast::Pattern::Normal(ast::Expr::Ident(ident)) => f.write_str(ident.as_str()),
            ast::Pattern::Normal(_) => f.write_str("?"), // unreachable?
            ast::Pattern::Placeholder(_) => f.write_str("_"),
            ast::Pattern::Parenthesized(p) => {
                write!(f, "{}", PatternDisplay(&p.pattern()))
            }
            ast::Pattern::Destructuring(d) => {
                write!(f, "(")?;
                let mut first = true;
                for item in d.items() {
                    if first {
                        first = false;
                    } else {
                        write!(f, ", ")?;
                    }
                    match item {
                        ast::DestructuringItem::Pattern(p) => write!(f, "{}", PatternDisplay(&p))?,
                        ast::DestructuringItem::Named(n) => write!(
                            f,
                            "{}: {}",
                            n.name().as_str(),
                            unwrap_expr(n.expr()).to_untyped().text()
                        )?,
                        ast::DestructuringItem::Spread(s) => write!(
                            f,
                            "..{}",
                            s.sink_ident().map(|i| i.as_str()).unwrap_or_default()
                        )?,
                    }
                }
                write!(f, ")")?;
                Ok(())
            }
        }
    }
}

fn unwrap_expr(mut e: ast::Expr) -> ast::Expr {
    while let ast::Expr::Parenthesized(p) = e {
        e = p.expr();
    }

    e
}
