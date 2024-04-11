//! Analysis of function signatures.
use core::fmt;
use std::{borrow::Cow, collections::HashMap, ops::Range, sync::Arc};

use ecow::{eco_format, eco_vec, EcoString, EcoVec};
use itertools::Itertools;
use log::trace;
use typst::syntax::{FileId as TypstFileId, Source};
use typst::{
    foundations::{CastInfo, Closure, Func, ParamInfo, Repr, Value},
    syntax::{
        ast::{self, AstNode},
        LinkedNode, Span, SyntaxKind,
    },
    util::LazyHash,
};

use crate::AnalysisContext;

// pub fn analyze_signature

/// Describes a function parameter.
#[derive(Debug, Clone)]
pub struct ParamSpec {
    /// The parameter's name.
    pub name: Cow<'static, str>,
    /// Documentation for the parameter.
    pub docs: Cow<'static, str>,
    /// Describe what values this parameter accepts.
    pub input: CastInfo,
    /// The parameter's default name as type.
    pub type_repr: Option<EcoString>,
    /// The parameter's default name as value.
    pub expr: Option<EcoString>,
    /// Creates an instance of the parameter's default value.
    pub default: Option<fn() -> Value>,
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

impl ParamSpec {
    fn from_static(s: &ParamInfo) -> Arc<Self> {
        Arc::new(Self {
            name: Cow::Borrowed(s.name),
            docs: Cow::Borrowed(s.docs),
            input: s.input.clone(),
            type_repr: Some(eco_format!("{}", TypeExpr(&s.input))),
            expr: None,
            default: s.default,
            positional: s.positional,
            named: s.named,
            variadic: s.variadic,
            settable: s.settable,
        })
    }
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
}

/// Describes a primary function signature.
#[derive(Debug, Clone)]
pub struct PrimarySignature {
    /// The positional parameters.
    pub pos: Vec<Arc<ParamSpec>>,
    /// The named parameters.
    pub named: HashMap<Cow<'static, str>, Arc<ParamSpec>>,
    /// Whether the function has fill, stroke, or size parameters.
    pub has_fill_or_size_or_stroke: bool,
    /// The rest parameter.
    pub rest: Option<Arc<ParamSpec>>,
    _broken: bool,
}

/// Describes a function argument instance
#[derive(Debug, Clone)]
pub struct ArgInfo {
    /// The argument's name.
    pub name: Option<EcoString>,
    /// The argument's value.
    pub value: Option<Value>,
}

/// Describes a span.
#[derive(Debug, Clone)]
pub enum SpanInfo {
    /// Unresolved raw span
    Span(Span),
    /// Resolved span
    Range((TypstFileId, Range<usize>)),
}

/// Describes a function argument list.
#[derive(Debug, Clone)]
pub struct ArgsInfo {
    /// The span of the argument list.
    pub span: Option<SpanInfo>,
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
pub enum SignatureTarget<'a> {
    /// A static node without knowing the function at runtime.
    Syntax(LinkedNode<'a>),
    /// A function that is known at runtime.
    Runtime(Func),
}

pub(crate) fn analyze_signature(ctx: &mut AnalysisContext, func: Func) -> Signature {
    ctx.analysis
        .caches
        .compute_signature(None, SignatureTarget::Runtime(func.clone()), || {
            Signature::Primary(analyze_dyn_signature(func))
        })
}

pub(crate) fn analyze_signature_v2(
    ctx: &mut AnalysisContext,
    source: Source,
    callee_node: SignatureTarget,
) -> Option<Signature> {
    if let Some(sig) = ctx.analysis.caches.signature(Some(source), &callee_node) {
        return Some(sig);
    }

    let func = match callee_node {
        SignatureTarget::Syntax(node) => {
            let values = crate::analysis::analyze_expr(ctx.world(), &node);
            let func = values.into_iter().find_map(|v| match v.0 {
                Value::Func(f) => Some(f),
                _ => None,
            })?;
            log::debug!("got function {func:?}");

            func
        }
        SignatureTarget::Runtime(func) => func,
    };

    use typst::foundations::func::Repr;
    let mut with_stack = eco_vec![];
    let mut func = func;
    while let Repr::With(f) = func.inner() {
        with_stack.push(ArgsInfo {
            span: None,
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

    let signature = ctx
        .analysis
        .caches
        .compute_signature(None, SignatureTarget::Runtime(func.clone()), || {
            Signature::Primary(analyze_dyn_signature(func))
        })
        .primary()
        .clone();
    trace!("got signature {signature:?}");

    if with_stack.is_empty() {
        return Some(Signature::Primary(signature));
    }

    Some(Signature::Partial(Arc::new(PartialSignature {
        signature,
        with_stack,
    })))
}

pub(crate) fn analyze_dyn_signature(func: Func) -> Arc<PrimarySignature> {
    use typst::foundations::func::Repr;
    let params = match func.inner() {
        Repr::With(..) => unreachable!(),
        Repr::Closure(c) => analyze_closure_signature(c.clone()),
        Repr::Element(..) | Repr::Native(..) => {
            let params = func.params().unwrap();
            params.iter().map(ParamSpec::from_static).collect()
        }
    };

    let mut pos = vec![];
    let mut named = HashMap::new();
    let mut rest = None;
    let mut broken = false;
    let mut has_fill = false;
    let mut has_stroke = false;
    let mut has_size = false;

    for param in params.into_iter() {
        if param.named {
            match param.name.as_ref() {
                "fill" => {
                    has_fill = true;
                }
                "stroke" => {
                    has_stroke = true;
                }
                "size" => {
                    has_size = true;
                }
                _ => {}
            }
            named.insert(param.name.clone(), param.clone());
        }

        if param.variadic {
            if rest.is_some() {
                broken = true;
            } else {
                rest = Some(param.clone());
            }
        }

        if param.positional {
            pos.push(param);
        }
    }

    Arc::new(PrimarySignature {
        pos,
        named,
        rest,
        has_fill_or_size_or_stroke: has_fill || has_stroke || has_size,
        _broken: broken,
    })
}

fn analyze_closure_signature(c: Arc<LazyHash<Closure>>) -> Vec<Arc<ParamSpec>> {
    let mut params = vec![];

    trace!("closure signature for: {:?}", c.node.kind());

    let closure = &c.node;
    let closure_ast = match closure.kind() {
        SyntaxKind::Closure => closure.cast::<ast::Closure>().unwrap(),
        _ => return params,
    };

    for param in closure_ast.params().children() {
        match param {
            ast::Param::Pos(ast::Pattern::Placeholder(..)) => {
                params.push(Arc::new(ParamSpec {
                    name: Cow::Borrowed("_"),
                    input: CastInfo::Any,
                    type_repr: None,
                    expr: None,
                    default: None,
                    positional: true,
                    named: false,
                    variadic: false,
                    settable: false,
                    docs: Cow::Borrowed(""),
                }));
            }
            ast::Param::Pos(e) => {
                // todo: destructing
                let name = e.bindings();
                if name.len() != 1 {
                    continue;
                }
                let name = name[0].as_str();

                params.push(Arc::new(ParamSpec {
                    name: Cow::Owned(name.to_owned()),
                    input: CastInfo::Any,
                    type_repr: None,
                    expr: None,
                    default: None,
                    positional: true,
                    named: false,
                    variadic: false,
                    settable: false,
                    docs: Cow::Borrowed(""),
                }));
            }
            // todo: pattern
            ast::Param::Named(n) => {
                let expr = unwrap_expr(n.expr()).to_untyped().clone().into_text();
                params.push(Arc::new(ParamSpec {
                    name: Cow::Owned(n.name().as_str().to_owned()),
                    input: CastInfo::Any,
                    type_repr: Some(expr.clone()),
                    expr: Some(expr.clone()),
                    default: None,
                    positional: false,
                    named: true,
                    variadic: false,
                    settable: true,
                    docs: Cow::Owned("Default value: ".to_owned() + expr.as_str()),
                }));
            }
            ast::Param::Spread(n) => {
                let ident = n.sink_ident().map(|e| e.as_str());
                params.push(Arc::new(ParamSpec {
                    name: Cow::Owned(ident.unwrap_or_default().to_owned()),
                    input: CastInfo::Any,
                    type_repr: None,
                    expr: None,
                    default: None,
                    positional: false,
                    named: false,
                    variadic: true,
                    settable: false,
                    docs: Cow::Borrowed(""),
                }));
            }
        }
    }

    params
}

fn unwrap_expr(mut e: ast::Expr) -> ast::Expr {
    while let ast::Expr::Parenthesized(p) = e {
        e = p.expr();
    }

    e
}

struct TypeExpr<'a>(&'a CastInfo);

impl<'a> fmt::Display for TypeExpr<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.0 {
            CastInfo::Any => "any",
            CastInfo::Value(v, _doc) => return write!(f, "{}", v.repr()),
            CastInfo::Type(v) => {
                f.write_str(v.short_name())?;
                return Ok(());
            }
            CastInfo::Union(v) => {
                let mut values = v.iter().map(|e| TypeExpr(e).to_string());
                f.write_str(&values.join(" | "))?;
                return Ok(());
            }
        })
    }
}
