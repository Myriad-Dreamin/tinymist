//! Analysis of function signatures.

use core::fmt;
use std::collections::BTreeMap;
use std::sync::Arc;

use ecow::{eco_format, eco_vec, EcoString, EcoVec};
use typst::foundations::{Closure, Func};
use typst::syntax::ast::AstNode;
use typst::syntax::{ast, SyntaxKind};
use typst::utils::LazyHash;

// use super::{BoundChecker, Definition};
use crate::ty::{InsTy, ParamTy, SigTy, StrRef, Ty};
use crate::ty::{Interned, ParamAttrs};
use crate::upstream::truncated_repr;
// use crate::upstream::truncated_repr;

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

    /// Returns the all parameters of the signature.
    pub fn params(&self) -> impl Iterator<Item = (&Interned<ParamTy>, Option<&Ty>)> {
        let primary = self.primary().params();
        // todo: with stack
        primary
    }

    /// Returns the type of the signature.
    pub fn type_sig(&self) -> Interned<SigTy> {
        let primary = self.primary().sig_ty.clone();
        // todo: with stack
        primary
    }

    /// Returns the shift applied to the signature.
    pub fn param_shift(&self) -> usize {
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
    pub sig_ty: Interned<SigTy>,
    /// Whether the signature is broken.
    pub _broken: bool,
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
            .map(|(idx, pos)| (pos, type_sig.pos(idx)));
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

/// Gets the signature of a function.
#[comemo::memoize]
pub fn func_signature(func: Func) -> Signature {
    use typst::foundations::func::Repr;
    let mut with_stack = eco_vec![];
    let mut func = func;
    while let Repr::With(with) = func.inner() {
        let (inner, args) = with.as_ref();
        with_stack.push(ArgsInfo {
            items: args
                .items
                .iter()
                .map(|arg| ArgInfo {
                    name: arg.name.clone().map(From::from),
                    term: Some(Ty::Value(InsTy::new(arg.value.v.clone()))),
                })
                .collect(),
        });
        func = inner.clone();
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
        Repr::Closure(closure) => {
            analyze_closure_signature(closure.clone(), &mut add_param);
            None
        }
        Repr::Element(..) | Repr::Native(..) | Repr::Plugin(..) => {
            for param in func.params().unwrap_or_default() {
                add_param(Interned::new(ParamTy {
                    name: param.name.into(),
                    docs: Some(param.docs.into()),
                    default: param.default.map(|default| truncated_repr(&default())),
                    ty: Ty::from_param_site(&func, param),
                    attrs: param.into(),
                }));
            }

            func.returns().map(|r| Ty::from_return_site(&func, r))
        }
    };

    let sig_ty = SigTy::new(pos_tys.into_iter(), named_tys, None, rest_ty, ret_ty);

    for name in &sig_ty.names.names {
        let Some(param) = named_specs.get(name) else {
            continue;
        };
        param_specs.push(param.clone());
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
    closure: Arc<LazyHash<Closure>>,
    add_param: &mut impl FnMut(Interned<ParamTy>),
) {
    log::trace!("closure signature for: {:?}", closure.node.kind());

    let closure = &closure.node;
    let closure_ast = match closure.kind() {
        SyntaxKind::Closure => closure.cast::<ast::Closure>().unwrap(),
        _ => return,
    };

    for param in closure_ast.params().children() {
        match param {
            ast::Param::Pos(pos) => {
                let name = format!("{}", PatternDisplay(&pos));
                add_param(Interned::new(ParamTy {
                    name: name.as_str().into(),
                    docs: None,
                    default: None,
                    ty: Ty::Any,
                    attrs: ParamAttrs::positional(),
                }));
            }
            // todo: pattern
            ast::Param::Named(named) => {
                let default = unwrap_parens(named.expr()).to_untyped().clone().into_text();
                add_param(Interned::new(ParamTy {
                    name: named.name().get().into(),
                    docs: Some(eco_format!("Default value: {default}")),
                    default: Some(default),
                    ty: Ty::Any,
                    attrs: ParamAttrs::named(),
                }));
            }
            ast::Param::Spread(spread) => {
                let sink = spread.sink_ident().map(|sink| sink.as_str());
                add_param(Interned::new(ParamTy {
                    name: sink.unwrap_or_default().into(),
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
            ast::Pattern::Parenthesized(paren_expr) => {
                write!(f, "{}", PatternDisplay(&paren_expr.pattern()))
            }
            ast::Pattern::Destructuring(destructing) => {
                write!(f, "(")?;
                let mut first = true;
                for item in destructing.items() {
                    if first {
                        first = false;
                    } else {
                        write!(f, ", ")?;
                    }
                    match item {
                        ast::DestructuringItem::Pattern(pos) => {
                            write!(f, "{}", PatternDisplay(&pos))?
                        }
                        ast::DestructuringItem::Named(named) => write!(
                            f,
                            "{}: {}",
                            named.name().as_str(),
                            unwrap_parens(named.expr()).to_untyped().text()
                        )?,
                        ast::DestructuringItem::Spread(spread) => write!(
                            f,
                            "..{}",
                            spread
                                .sink_ident()
                                .map(|sink| sink.as_str())
                                .unwrap_or_default()
                        )?,
                    }
                }
                write!(f, ")")?;
                Ok(())
            }
        }
    }
}

fn unwrap_parens(mut expr: ast::Expr) -> ast::Expr {
    while let ast::Expr::Parenthesized(paren_expr) = expr {
        expr = paren_expr.expr();
    }

    expr
}
