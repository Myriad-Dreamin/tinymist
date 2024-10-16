//! Analysis of function signatures.
use core::fmt;
use std::collections::BTreeMap;
use std::{borrow::Cow, sync::Arc};

use ecow::{eco_vec, EcoString, EcoVec};
use log::trace;
use typst::syntax::Source;
use typst::{
    foundations::{Closure, Func, ParamInfo, Value},
    syntax::{
        ast::{self, AstNode},
        LinkedNode, SyntaxKind,
    },
};
use typst_shim::syntax::LinkedNodeExt;
use typst_shim::utils::LazyHash;

use crate::adt::interner::Interned;
use crate::analysis::{resolve_callee, DocString};
use crate::syntax::{get_def_target, get_deref_target, DefTarget};
use crate::ty::SigTy;
use crate::upstream::truncated_repr;
use crate::AnalysisContext;

use super::{
    find_definition, DefinitionLink, IdentRef, LexicalKind, LexicalVarKind, StrRef, Ty,
    TypeInterface, VarDoc,
};

// pub fn analyze_signature
/// Attributes of a function parameter.
#[derive(Debug, Clone, Copy)]
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

/// Parameter specification for a function.
#[derive(Clone)]
pub struct ParamSpecShort {
    /// The signature of the function.
    pub sig: Arc<PrimarySignature>,
    /// The offset of the parameter in the signature.
    pub offset: u32,
    /// The attributes of the parameter.
    pub attr: ParamAttrs,
}

impl fmt::Debug for ParamSpecShort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParamSpecShort")
            .field("name", self.name())
            .field("offset", &self.offset)
            .field("attr", &self.attr)
            .finish()
    }
}

impl ParamSpecShort {
    /// Returns the name of the parameter.
    pub fn name(&self) -> &StrRef {
        if self.attr.named {
            &self.sig.named_names()[(self.offset - self.sig.sig_ty.name_started) as usize]
        } else if self.attr.variadic {
            self.sig.rest_name().unwrap()
        } else {
            &self.sig.pos_names()[self.offset as usize]
        }
    }

    /// Returns the documentation of the parameter.
    pub fn docstring(&self) -> Option<&VarDoc> {
        self.sig.docs.get_var(self.name())
    }

    /// Returns the type of the parameter.
    pub fn ty(&self) -> &Ty {
        if self.attr.named {
            self.sig
                .sig_ty
                .field_by_bone_offset(self.offset as usize)
                .unwrap()
        } else if self.attr.variadic {
            self.sig.sig_ty.rest_param().unwrap()
        } else {
            self.sig.sig_ty.pos(self.offset as usize).unwrap()
        }
    }
}

/// Long parameter specification for a function.
#[derive(Debug, Clone)]
pub struct ParamSpecLong<'a> {
    /// The name of the parameter.
    pub name: &'a StrRef,
    /// The docstring of the parameter.
    pub docstring: Option<&'a VarDoc>,
    /// The type of the parameter.
    pub ty: &'a Ty,
    /// The attributes of the parameter.
    pub attrs: ParamAttrs,
}

impl ParamSpecLong<'_> {
    /// Returns the name of the parameter.
    pub fn name(&self) -> &StrRef {
        self.name
    }

    /// Returns the documentation of the parameter.
    pub fn docs(&self) -> Option<&EcoString> {
        self.docstring.and_then(|d| d.docs.as_ref())
    }

    /// Returns the type of the parameter.
    pub fn ty(&self) -> &Ty {
        self.ty
    }
}

/// Describes a function parameter.
#[derive(Debug, Clone)]
pub struct ParamSpec {
    /// The parameter's name.
    pub name: Interned<str>,
    /// Documentation for the parameter.
    pub docs: Cow<'static, str>,
    /// Inferred type of the parameter.
    pub(crate) base_type: Ty,
    /// The parameter's default name as value.
    pub expr: Option<EcoString>,
    /// The attributes of the parameter.
    pub attrs: ParamAttrs,
}

impl ParamSpec {
    fn from_static(f: &Func, p: &ParamInfo) -> Arc<Self> {
        Arc::new(Self {
            name: p.name.into(),
            docs: Cow::Borrowed(p.docs),
            base_type: Ty::from_param_site(f, p),
            expr: p.default.map(|d| truncated_repr(&d())),
            attrs: ParamAttrs {
                positional: p.positional,
                named: p.named,
                variadic: p.variadic,
                settable: p.settable,
            },
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

    pub(crate) fn type_sig(&self) -> Interned<SigTy> {
        let primary = self.primary().sig_ty.clone();
        // todo: with stack
        primary
    }
}

/// Describes a primary function signature.
#[derive(Debug, Clone)]
pub struct PrimarySignature {
    /// Documentation for the function.
    pub docs: DocString,
    /// The attributes of the parameters.
    pub attrs: Vec<ParamAttrs>,
    /// The name of positional and rest parameters.
    pub pos_rest_names: Vec<Interned<str>>,
    /// Whether the function has fill, stroke, or size parameters.
    pub has_fill_or_size_or_stroke: bool,
    /// The signature type.
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

    /// Returns names of positional parameters of the function.
    pub fn pos_names(&self) -> &[StrRef] {
        &self.pos_rest_names[..self.pos_size()]
    }

    /// Returns the positional parameters of the function.
    pub fn pos(&self) -> impl Iterator<Item = ParamSpecLong> {
        self.pos_names()
            .iter()
            .enumerate()
            .map(|(i, name)| ParamSpecLong {
                name,
                docstring: self.docs.get_var(name),
                ty: self.sig_ty.pos(i).unwrap(),
                attrs: self.attrs[i],
            })
    }

    /// Returns the names of the named parameter of the function.
    pub fn named_names(&self) -> &[StrRef] {
        &self.sig_ty.names.names
    }

    /// Returns the named parameters of the function.
    pub fn named(&self) -> impl Iterator<Item = ParamSpecLong> {
        self.named_names()
            .iter()
            .enumerate()
            .map(move |(i, name)| ParamSpecLong {
                name,
                docstring: self.docs.get_var(name),
                ty: self.sig_ty.field_by_bone_offset(i).unwrap(),
                attrs: self.attrs[i + self.pos_size()],
            })
    }

    /// Returns the name of the rest parameter of the function.
    pub fn rest_name(&self) -> Option<&StrRef> {
        (self.pos_rest_names.len() > self.sig_ty.name_started as usize)
            .then(|| self.pos_rest_names.last().unwrap())
    }

    /// Returns the rest parameter of the function.
    pub fn rest(&self) -> Option<ParamSpecLong> {
        // (self.pos_names.len() > self.sig_ty.name_started as usize)
        //     .then(|| self.docs.get_var(&self.pos_names.last().unwrap()))
        //     .flatten()
        let name = self.rest_name()?;
        Some(ParamSpecLong {
            name,
            docstring: self.docs.get_var(name),
            ty: self.sig_ty.rest_param().unwrap(),
            attrs: self.attrs[self.pos_size() + self.sig_ty.names.names.len()],
        })
    }

    /// Returns the positional parameters of the function.
    pub fn get_pos(self: &Arc<Self>, offset: usize) -> Option<ParamSpecShort> {
        (offset < self.pos_size()).then(|| ParamSpecShort {
            sig: self.clone(),
            offset: offset as u32,
            attr: self.attrs[offset],
        })
    }

    /// Returns the named parameters of the function.
    pub fn get_named(self: &Arc<Self>, name: &StrRef) -> Option<ParamSpecShort> {
        let offset = self.sig_ty.names.find(name)?;
        let offset = self.sig_ty.name_started as usize + offset;
        Some(ParamSpecShort {
            sig: self.clone(),
            offset: offset as u32,
            attr: self.attrs[offset],
        })
    }

    /// Returns the rest parameter of the function.
    pub fn get_rest(self: &Arc<Self>) -> Option<ParamSpecShort> {
        (self.pos_rest_names.len() > self.sig_ty.name_started as usize).then(|| {
            let offset = self.pos_rest_names.len();
            ParamSpecShort {
                sig: self.clone(),
                offset: offset as u32,
                attr: *self.attrs.last().unwrap(),
            }
        })
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
pub enum SignatureTarget<'a> {
    /// A static node without knowing the function at runtime.
    Def(Source, IdentRef),
    /// A static node without knowing the function at runtime.
    Syntax(Source, LinkedNode<'a>),
    /// A function that is known at runtime.
    Runtime(Func),
}

pub(crate) fn analyze_dyn_signature(ctx: &mut AnalysisContext, func: Func) -> Signature {
    ctx.compute_signature(SignatureTarget::Runtime(func.clone()), || {
        Signature::Primary(analyze_dyn_signature_inner(func))
    })
}

pub(crate) fn analyze_signature(
    ctx: &mut AnalysisContext,
    callee_node: SignatureTarget,
) -> Option<Signature> {
    if let Some(sig) = ctx.signature(&callee_node) {
        return Some(sig);
    }

    let func = match callee_node {
        SignatureTarget::Def(..) => todo!(),
        SignatureTarget::Syntax(source, node) => {
            let _ = resolve_callee_v2;
            let _ = source;

            // let res = resolve_callee_v2(ctx, node)?;

            // let func = match res {
            //     TryResolveCalleeResult::Syntax(lnk) => {
            //         println!("Syntax {:?}", lnk.name);

            //         return analyze_static_signature(ctx, source, lnk);
            //     }
            //     TryResolveCalleeResult::Runtime(func) => func,
            // };

            let func = resolve_callee(ctx, &node)?;

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
        .compute_signature(SignatureTarget::Runtime(func.clone()), || {
            Signature::Primary(analyze_dyn_signature_inner(func))
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

// fn analyze_static_signature(
//     ctx: &mut AnalysisContext<'_>,
//     source: Source,
//     lnk: DefinitionLink,
// ) -> Option<Signature> {
//     let def_at = lnk.def_at?;
//     let def_source = if def_at.0 == source.id() {
//         source.clone()
//     } else {
//         ctx.source_by_id(def_at.0).ok()?
//     };

//     let root = LinkedNode::new(def_source.root());
//     let def_node = root.leaf_at_compat(def_at.1.start + 1)?;
//     let def_node = get_def_target(def_node)?;
//     let def_node = match def_node {
//         DefTarget::Let(node) => node,
//         DefTarget::Import(_) => return None,
//     };

//     println!("def_node {def_node:?}");

//     None
// }

#[allow(dead_code)]
enum TryResolveCalleeResult {
    Syntax(DefinitionLink),
    Runtime(Func),
}

/// Resolve a callee expression to a function but prefer to keep static.
fn resolve_callee_v2(
    ctx: &mut AnalysisContext,
    callee: LinkedNode,
) -> Option<TryResolveCalleeResult> {
    let source = ctx.source_by_id(callee.span().id()?).ok()?;
    let node = source.find(callee.span())?;
    let cursor = node.offset();
    let deref_target = get_deref_target(node, cursor)?;
    let def = find_definition(ctx, source.clone(), None, deref_target)?;
    if let LexicalKind::Var(LexicalVarKind::Function) = def.kind {
        if let Some(Value::Func(f)) = def.value {
            return Some(TryResolveCalleeResult::Runtime(f));
        }
    }

    if let Some(def_at) = &def.def_at {
        let def_source = if def_at.0 == source.id() {
            source.clone()
        } else {
            ctx.source_by_id(def_at.0).ok()?
        };

        let _t = ctx.type_check(source)?;

        let root = LinkedNode::new(def_source.root());
        let def_node = root.leaf_at_compat(def_at.1.start + 1)?;
        let def_node = get_def_target(def_node)?;
        let _def_node = match def_node {
            DefTarget::Let(node) => node,
            DefTarget::Import(_) => return None,
        };
    }

    Some(TryResolveCalleeResult::Syntax(def))
}

fn analyze_dyn_signature_inner(func: Func) -> Arc<PrimarySignature> {
    use typst::foundations::func::Repr;
    let (params, ret_ty) = match func.inner() {
        Repr::With(..) => unreachable!(),
        Repr::Closure(c) => (analyze_closure_signature(c.clone()), None),
        Repr::Element(..) | Repr::Native(..) => {
            let ret_ty = func.returns().map(|r| Ty::from_return_site(&func, r));
            let params = func.params().unwrap();
            (
                params
                    .iter()
                    .map(|p| ParamSpec::from_static(&func, p))
                    .collect(),
                ret_ty,
            )
        }
    };

    let mut docs = DocString::default();
    let mut pos = vec![];
    let mut named_vec = Vec::new();
    let mut pos_names = vec![];
    let mut named_attrs = BTreeMap::new();
    let mut pos_attrs = Vec::new();
    let mut rest = None;
    let mut rest_attr = None;
    let mut broken = false;
    let mut has_fill = false;
    let mut has_stroke = false;
    let mut has_size = false;

    for param in params.into_iter() {
        let name = param.name.clone();
        docs.vars.insert(
            name.clone(),
            VarDoc {
                docs: Some(param.docs.as_ref().into()),
                ty: Some(param.base_type.clone()),
                default: param.expr.clone(),
            },
        );
        if param.attrs.named {
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
            named_vec.push((name.clone(), param.base_type.clone()));
            named_attrs.insert(name.clone(), param.attrs);
        }

        if param.attrs.variadic {
            if rest.is_some() {
                broken = true;
            } else {
                rest = Some(param.clone());
                pos_names.push(name);
                rest_attr = Some(param.attrs);
            }
        } else if param.attrs.positional {
            pos_names.push(name);
            pos_attrs.push(param.attrs);
            pos.push(param);
        }
    }

    // let mut named_vec: Vec<(Interned<str>, Ty)> = named
    //     .iter()
    //     .map(|e| (e.0.clone(), e.1.base_type.clone()))
    //     .collect::<Vec<_>>();

    let sig_ty = SigTy::new(
        pos.iter().map(|e| e.base_type.clone()),
        named_vec,
        None,
        rest.as_ref().map(|e| e.base_type.clone()),
        ret_ty.clone(),
    );

    for name in &sig_ty.names.names {
        pos_attrs.push(*named_attrs.get(name).unwrap_or(&ParamAttrs {
            positional: false,
            named: true,
            variadic: false,
            settable: false,
        }));
    }

    if rest.is_some() {
        pos_attrs.push(rest_attr.unwrap());
    }

    Arc::new(PrimarySignature {
        docs,
        attrs: pos_attrs,
        pos_rest_names: pos_names,
        // pos,
        // named,
        // rest,
        // ret_ty,
        has_fill_or_size_or_stroke: has_fill || has_stroke || has_size,
        sig_ty: sig_ty.into(),
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
            ast::Param::Pos(e) => {
                let name = format!("{}", PatternDisplay(&e));

                params.push(Arc::new(ParamSpec {
                    name: name.as_str().into(),
                    base_type: Ty::Any,
                    // type_repr: None,
                    expr: None,
                    attrs: ParamAttrs {
                        positional: true,
                        named: false,
                        variadic: false,
                        settable: false,
                    },
                    docs: Cow::Borrowed(""),
                }));
            }
            // todo: pattern
            ast::Param::Named(n) => {
                let expr = unwrap_expr(n.expr()).to_untyped().clone().into_text();
                params.push(Arc::new(ParamSpec {
                    name: n.name().into(),
                    base_type: Ty::Any,
                    expr: Some(expr.clone()),
                    attrs: ParamAttrs {
                        positional: false,
                        named: true,
                        variadic: false,
                        settable: true,
                    },
                    docs: Cow::Owned("Default value: ".to_owned() + expr.as_str()),
                }));
            }
            ast::Param::Spread(n) => {
                let ident = n.sink_ident().map(|e| e.as_str());
                params.push(Arc::new(ParamSpec {
                    name: ident.unwrap_or_default().into(),
                    base_type: Ty::Any,
                    expr: None,
                    attrs: ParamAttrs {
                        positional: true,
                        named: false,
                        variadic: true,
                        settable: false,
                    },
                    docs: Cow::Borrowed(""),
                }));
            }
        }
    }

    params
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
