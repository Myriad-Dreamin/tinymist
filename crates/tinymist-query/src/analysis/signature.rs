//! Analysis of function signatures.

use typst::foundations::{self, Closure, ParamInfo};

use super::{prelude::*, resolve_callee, BuiltinTy, SigTy, TypeSources};
use crate::analysis::PostTypeChecker;
use crate::docs::UntypedSymbolDocs;
use crate::syntax::get_non_strict_def_target;
use crate::upstream::truncated_repr;

/// Describes a function parameter.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct ParamAttrs {
    /// Is the parameter positional?
    pub positional: bool,
    /// Is the parameter named?
    ///
    /// Can be true even if `positional` is true if the parameter can be given
    /// in both variants.
    pub named: bool,
    /// Can the parameter be given any number of times?
    pub variadic: bool,
    /// Is the parameter settable with a set rule?
    pub settable: bool,
}

impl ParamAttrs {
    pub(crate) fn positional() -> ParamAttrs {
        ParamAttrs {
            positional: true,
            named: false,
            variadic: false,
            settable: false,
        }
    }

    pub(crate) fn named() -> ParamAttrs {
        ParamAttrs {
            positional: false,
            named: true,
            variadic: false,
            settable: false,
        }
    }

    pub(crate) fn variadic() -> ParamAttrs {
        ParamAttrs {
            positional: true,
            named: false,
            variadic: true,
            settable: false,
        }
    }
}

impl From<&ParamInfo> for ParamAttrs {
    fn from(param: &ParamInfo) -> Self {
        ParamAttrs {
            positional: param.positional,
            named: param.named,
            variadic: param.variadic,
            settable: param.settable,
        }
    }
}

/// Describes a function parameter.
#[derive(Debug, Clone)]
pub struct ParamSpec {
    /// The name of the parameter.
    pub name: StrRef,
    /// The docstring of the parameter.
    pub docs: Option<EcoString>,
    /// The default value of the variable
    pub default: Option<EcoString>,
    /// The type of the parameter.
    pub ty: Ty,
    /// The attributes of the parameter.
    pub attrs: ParamAttrs,
}

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
    pub(crate) fn params(&self) -> impl Iterator<Item = (&ParamSpec, Option<&Ty>)> {
        let primary = self.primary().params();
        // todo: with stack
        primary
    }

    pub(crate) fn type_sig(&self) -> Interned<SigTy> {
        let primary = self.primary().sig_ty.clone();
        // todo: with stack
        primary
    }
}

/// Describes a primary function signature.
#[derive(Debug, Clone)]
pub struct PrimarySignature {
    /// The documentation of the function
    pub docs: Option<EcoString>,
    /// The documentation of the parameter.
    pub param_specs: Vec<ParamSpec>,
    /// Whether the function has fill, stroke, or size parameters.
    pub has_fill_or_size_or_stroke: bool,
    /// The associated signature type.
    pub(crate) sig_ty: Interned<SigTy>,
    _broken: bool,
}

impl PrimarySignature {
    /// Returns the type representation of the function.
    pub(crate) fn ty(&self) -> Ty {
        Ty::Func(self.sig_ty.clone())
    }

    /// Returns the number of positional parameters of the function.
    pub fn pos_size(&self) -> usize {
        self.sig_ty.name_started as usize
    }

    /// Returns the positional parameters of the function.
    pub fn pos(&self) -> &[ParamSpec] {
        &self.param_specs[..self.pos_size()]
    }

    /// Returns the positional parameters of the function.
    pub fn get_pos(&self, offset: usize) -> Option<&ParamSpec> {
        self.pos().get(offset)
    }

    /// Returns the named parameters of the function.
    pub fn named(&self) -> &[ParamSpec] {
        &self.param_specs[self.pos_size()..self.pos_size() + self.sig_ty.names.names.len()]
    }

    /// Returns the named parameters of the function.
    pub fn get_named(&self, name: &StrRef) -> Option<&ParamSpec> {
        self.named().get(self.sig_ty.names.find(name)?)
    }

    /// Returns the name of the rest parameter of the function.
    pub fn has_spread_right(&self) -> bool {
        self.sig_ty.spread_right
    }

    /// Returns the rest parameter of the function.
    pub fn rest(&self) -> Option<&ParamSpec> {
        self.has_spread_right()
            .then(|| &self.param_specs[self.pos_size() + self.sig_ty.names.names.len()])
    }

    /// Returns the all parameters of the function.
    pub fn params(&self) -> impl Iterator<Item = (&ParamSpec, Option<&Ty>)> {
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
    pub name: Option<EcoString>,
    /// The argument's value.
    pub value: Option<Value>,
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
pub enum SignatureTarget<'a> {
    /// A static node without knowing the function at runtime.
    Def(Source, IdentRef),
    /// A static node without knowing the function at runtime.
    SyntaxFast(Source, LinkedNode<'a>),
    /// A static node without knowing the function at runtime.
    Syntax(Source, LinkedNode<'a>),
    /// A function that is known at runtime.
    Runtime(Func),
    /// A function that is known at runtime.
    Convert(Func),
}

pub(crate) fn analyze_signature(
    ctx: &mut AnalysisContext,
    callee_node: SignatureTarget,
) -> Option<Signature> {
    ctx.compute_signature(callee_node.clone(), |ctx| {
        log::debug!("analyzing signature for {callee_node:?}");
        analyze_type_signature(ctx, &callee_node)
            .or_else(|| analyze_dyn_signature(ctx, &callee_node))
    })
}

fn analyze_type_signature(
    ctx: &mut AnalysisContext,
    callee_node: &SignatureTarget<'_>,
) -> Option<Signature> {
    let (type_info, ty) = match callee_node {
        SignatureTarget::Def(..) | SignatureTarget::Convert(..) => None,
        SignatureTarget::SyntaxFast(source, node) | SignatureTarget::Syntax(source, node) => {
            let type_info = ctx.type_check(source)?;
            let ty = type_info.type_of_span(node.span())?;
            Some((type_info, ty))
        }
        SignatureTarget::Runtime(f) => {
            let source = ctx.source_by_id(f.span().id()?).ok()?;
            let node = source.find(f.span())?;
            let def = get_non_strict_def_target(node.parent()?.clone())?;
            let type_info = ctx.type_check(&source)?;
            let ty = type_info.type_of_span(def.name()?.span())?;
            Some((type_info, ty))
        }
    }?;

    // todo multiple sources
    let mut srcs = ty.sources();
    srcs.sort();
    log::debug!("check type signature of ty: {ty:?} => {srcs:?}");
    let type_var = srcs.into_iter().next()?;
    match type_var {
        TypeSources::Var(v) => {
            let mut ty_ctx = PostTypeChecker::new(ctx, &type_info);
            let sig_ty = Ty::Func(ty.sig_repr(true, &mut ty_ctx)?);
            let sig_ty = type_info.simplify(sig_ty, false);
            let Ty::Func(sig_ty) = sig_ty else {
                panic!("expected function type, got {sig_ty:?}");
            };

            let docstring = match type_info.var_docs.get(&v.def).map(|x| x.as_ref()) {
                Some(UntypedSymbolDocs::Function(sig)) => sig,
                _ => return None,
            };

            let mut param_specs = Vec::new();
            let mut has_fill_or_size_or_stroke = false;
            let mut _broken = false;

            if docstring.pos.len() != sig_ty.positional_params().len() {
                panic!("positional params mismatch: {docstring:#?} != {sig_ty:#?}");
            }

            for (doc, ty) in docstring.pos.iter().zip(sig_ty.positional_params()) {
                let default = doc.default.clone();
                let ty = ty.clone();

                let name = doc.name.clone();
                if matches!(name.as_ref(), "fill" | "stroke" | "size") {
                    has_fill_or_size_or_stroke = true;
                }

                param_specs.push(ParamSpec {
                    name,
                    docs: Some(doc.docs.clone()),
                    default,
                    ty,
                    attrs: ParamAttrs::positional(),
                });
            }

            for (name, ty) in sig_ty.named_params() {
                let doc = docstring.named.get(name).unwrap();
                let default = doc.default.clone();
                let ty = ty.clone();

                if matches!(name.as_ref(), "fill" | "stroke" | "size") {
                    has_fill_or_size_or_stroke = true;
                }

                param_specs.push(ParamSpec {
                    name: name.clone(),
                    docs: Some(doc.docs.clone()),
                    default,
                    ty,
                    attrs: ParamAttrs::named(),
                });
            }

            if let Some(doc) = docstring.rest.as_ref() {
                let default = doc.default.clone();

                param_specs.push(ParamSpec {
                    name: doc.name.clone(),
                    docs: Some(doc.docs.clone()),
                    default,
                    ty: sig_ty.rest_param().cloned().unwrap_or(Ty::Any),
                    attrs: ParamAttrs::variadic(),
                });
            }

            Some(Signature::Primary(Arc::new(PrimarySignature {
                docs: Some(docstring.docs.clone()),
                param_specs,
                has_fill_or_size_or_stroke,
                sig_ty,
                _broken,
            })))
        }
        TypeSources::Builtin(BuiltinTy::Type(ty)) => {
            let cons = ty.constructor().ok()?;
            Some(ctx.type_of_func(cons))
        }
        TypeSources::Builtin(BuiltinTy::Element(ty)) => {
            let cons: Func = ty.into();
            Some(ctx.type_of_func(cons))
        }
        TypeSources::Builtin(..) => None,
        TypeSources::Ins(i) => match &i.val {
            foundations::Value::Func(f) => Some(ctx.type_of_func(f.clone())),
            foundations::Value::Type(f) => {
                let cons = f.constructor().ok()?;
                Some(ctx.type_of_func(cons))
            }
            _ => None,
        },
    }
}

fn analyze_dyn_signature(
    ctx: &mut AnalysisContext,
    callee_node: &SignatureTarget<'_>,
) -> Option<Signature> {
    let func = match callee_node {
        SignatureTarget::Def(..) => return None,
        SignatureTarget::SyntaxFast(..) => return None,
        SignatureTarget::Syntax(_, node) => {
            let func = resolve_callee(ctx, node)?;
            log::debug!("got function {func:?}");
            func
        }
        SignatureTarget::Convert(func) | SignatureTarget::Runtime(func) => func.clone(),
    };

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
                    value: Some(arg.value.v.clone()),
                })
                .collect(),
        });
        func = f.0.clone();
    }

    let signature = analyze_dyn_signature_inner(func);
    log::trace!("got signature {signature:?}");

    if with_stack.is_empty() {
        return Some(Signature::Primary(signature));
    }
    Some(Signature::Partial(Arc::new(PartialSignature {
        signature,
        with_stack,
    })))
}

fn analyze_dyn_signature_inner(func: Func) -> Arc<PrimarySignature> {
    let mut pos_tys = vec![];
    let mut named_tys = Vec::new();
    let mut rest_ty = None;

    let mut named_specs = BTreeMap::new();
    let mut param_specs = Vec::new();
    let mut rest_spec = None;

    let mut broken = false;
    let mut has_fill_or_size_or_stroke = false;

    let mut add_param = |param: ParamSpec| {
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

    use typst::foundations::func::Repr;
    let ret_ty = match func.inner() {
        Repr::With(..) => unreachable!(),
        Repr::Closure(c) => {
            analyze_closure_signature(c.clone(), &mut add_param);
            None
        }
        Repr::Element(..) | Repr::Native(..) => {
            for p in func.params().unwrap() {
                add_param(ParamSpec {
                    name: p.name.into(),
                    docs: Some(p.docs.into()),
                    default: p.default.map(|d| truncated_repr(&d())),
                    ty: Ty::from_param_site(&func, p),
                    attrs: p.into(),
                });
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

    Arc::new(PrimarySignature {
        docs: func.docs().map(From::from),
        param_specs,
        has_fill_or_size_or_stroke,
        sig_ty: sig_ty.into(),
        _broken: broken,
    })
}

fn analyze_closure_signature(c: Arc<LazyHash<Closure>>, add_param: &mut impl FnMut(ParamSpec)) {
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
                add_param(ParamSpec {
                    name: name.as_str().into(),
                    docs: None,
                    default: None,
                    ty: Ty::Any,
                    attrs: ParamAttrs::positional(),
                });
            }
            // todo: pattern
            ast::Param::Named(n) => {
                let expr = unwrap_expr(n.expr()).to_untyped().clone().into_text();
                add_param(ParamSpec {
                    name: n.name().get().into(),
                    docs: Some(eco_format!("Default value: {expr}")),
                    default: Some(expr),
                    ty: Ty::Any,
                    attrs: ParamAttrs::named(),
                });
            }
            ast::Param::Spread(n) => {
                let ident = n.sink_ident().map(|e| e.as_str());
                add_param(ParamSpec {
                    name: ident.unwrap_or_default().into(),
                    docs: None,
                    default: None,
                    ty: Ty::Any,
                    attrs: ParamAttrs::variadic(),
                });
            }
        }
    }
}

struct PatternDisplay<'a>(&'a ast::Pattern<'a>);

impl<'a> fmt::Display for PatternDisplay<'a> {
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
