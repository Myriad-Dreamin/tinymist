//! Analysis of function signatures.
use core::fmt;
use std::collections::BTreeMap;
use std::sync::Arc;

use ecow::{eco_format, eco_vec, EcoString, EcoVec};
use typst::syntax::Source;
use typst::{
    foundations::{Closure, Func, Value},
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

/// Describes a function parameter.
#[derive(Debug, Clone)]
pub struct ParamSpec<'a> {
    /// The name of the parameter.
    pub name: &'a StrRef,
    /// The docstring of the parameter.
    pub docs: Option<&'a EcoString>,
    /// The default value of the variable
    pub default: Option<&'a EcoString>,
    /// The type of the parameter.
    pub ty: &'a Ty,
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
    pub fn pos(&self) -> impl Iterator<Item = ParamSpec> {
        self.pos_names().iter().enumerate().map(|(i, name)| {
            let docstring = self.docs.get_var(name);
            ParamSpec {
                name,
                docs: docstring.and_then(|x| x.docs.as_ref()),
                default: docstring.and_then(|x| x.default.as_ref()),
                ty: self.sig_ty.pos(i).unwrap(),
                attrs: self.attrs[i],
            }
        })
    }

    /// Returns the names of the named parameter of the function.
    pub fn named_names(&self) -> &[StrRef] {
        &self.sig_ty.names.names
    }

    /// Returns the named parameters of the function.
    pub fn named(&self) -> impl Iterator<Item = ParamSpec> {
        self.named_names().iter().enumerate().map(move |(i, name)| {
            let docstring = self.docs.get_var(name);
            ParamSpec {
                name,
                docs: docstring.and_then(|x| x.docs.as_ref()),
                default: docstring.and_then(|x| x.default.as_ref()),
                ty: self.sig_ty.field_by_bone_offset(i).unwrap(),
                attrs: self.attrs[i + self.pos_size()],
            }
        })
    }

    /// Returns the name of the rest parameter of the function.
    pub fn rest_name(&self) -> Option<&StrRef> {
        (self.pos_rest_names.len() > self.sig_ty.name_started as usize)
            .then(|| self.pos_rest_names.last().unwrap())
    }

    /// Returns the rest parameter of the function.
    pub fn rest(&self) -> Option<ParamSpec> {
        let name = self.rest_name()?;
        let docstring = self.docs.get_var(name);
        Some(ParamSpec {
            name,
            docs: docstring.and_then(|x| x.docs.as_ref()),
            default: docstring.and_then(|x| x.default.as_ref()),
            ty: self.sig_ty.rest_param().unwrap(),
            attrs: self.attrs[self.pos_size() + self.sig_ty.names.names.len()],
        })
    }

    /// Returns the positional parameters of the function.
    pub fn get_pos(&self, offset: usize) -> Option<ParamSpec> {
        (offset < self.pos_size()).then(|| {
            let name = &self.pos_names()[offset];
            let docstring = self.docs.get_var(name);
            ParamSpec {
                name,
                docs: docstring.and_then(|x| x.docs.as_ref()),
                default: docstring.and_then(|x| x.default.as_ref()),
                ty: self.sig_ty.pos(offset).unwrap(),
                attrs: self.attrs[offset],
            }
        })
    }

    /// Returns the named parameters of the function.
    pub fn get_named(&self, name: &StrRef) -> Option<ParamSpec> {
        let offset = self.sig_ty.names.find(name)?;
        let name = &self.named_names()[offset];

        let offset = self.sig_ty.name_started as usize + offset;

        let docstring = self.docs.get_var(name);
        Some(ParamSpec {
            name,
            docs: docstring.and_then(|x| x.docs.as_ref()),
            default: docstring.and_then(|x| x.default.as_ref()),
            ty: self.sig_ty.field_by_bone_offset(offset).unwrap(),
            attrs: self.attrs[offset],
        })
    }

    /// Returns the rest parameter of the function.
    pub fn get_rest(&self) -> Option<ParamSpec> {
        (self.pos_rest_names.len() > self.sig_ty.name_started as usize).then(|| {
            let offset = self.pos_rest_names.len();
            let name = self.rest_name().unwrap();
            let docstring = self.docs.get_var(name);
            ParamSpec {
                name,
                docs: docstring.and_then(|x| x.docs.as_ref()),
                default: docstring.and_then(|x| x.default.as_ref()),
                ty: self.sig_ty.rest_param().unwrap(),
                attrs: self.attrs[offset],
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
    log::trace!("got signature {signature:?}");

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
    let mut docs = DocString::default();
    let mut pos = vec![];
    let mut named_vec = Vec::new();
    let mut pos_names = vec![];
    let mut named_attrs = BTreeMap::new();
    let mut pos_attrs = Vec::new();
    let mut rest_ty = None;
    let mut rest_attr = None;
    let mut broken = false;
    let mut has_fill = false;
    let mut has_stroke = false;
    let mut has_size = false;

    let mut add_param = |param: ParamSpec| {
        let name = param.name.clone();
        docs.vars.insert(
            name.clone(),
            VarDoc {
                docs: param.docs.cloned(),
                ty: Some(param.ty.clone()),
                default: param.default.cloned(),
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
            named_vec.push((name.clone(), param.ty.clone()));
            named_attrs.insert(name.clone(), param.attrs);
        }

        if param.attrs.variadic {
            if rest_ty.is_some() {
                broken = true;
            } else {
                rest_ty = Some(param.ty.clone());
                pos_names.push(name);
                rest_attr = Some(param.attrs);
            }
        } else if param.attrs.positional {
            pos_names.push(name);
            pos_attrs.push(param.attrs);
            pos.push(param.ty.clone());
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
                    name: &p.name.into(),
                    docs: Some(&p.docs.into()),
                    default: p.default.map(|d| truncated_repr(&d())).as_ref(),
                    ty: &Ty::from_param_site(&func, p),
                    attrs: ParamAttrs {
                        positional: p.positional,
                        named: p.named,
                        variadic: p.variadic,
                        settable: p.settable,
                    },
                });
            }

            func.returns().map(|r| Ty::from_return_site(&func, r))
        }
    };

    let sig_ty = SigTy::new(pos.into_iter(), named_vec, None, rest_ty, ret_ty);

    for name in &sig_ty.names.names {
        pos_attrs.push(*named_attrs.get(name).unwrap_or(&ParamAttrs {
            positional: false,
            named: true,
            variadic: false,
            settable: false,
        }));
    }
    if let Some(attr) = rest_attr {
        pos_attrs.push(attr);
    }

    Arc::new(PrimarySignature {
        docs,
        attrs: pos_attrs,
        pos_rest_names: pos_names,
        has_fill_or_size_or_stroke: has_fill || has_stroke || has_size,
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
                    name: &name.as_str().into(),
                    docs: None,
                    default: None,
                    ty: &Ty::Any,
                    attrs: ParamAttrs {
                        positional: true,
                        named: false,
                        variadic: false,
                        settable: false,
                    },
                });
            }
            // todo: pattern
            ast::Param::Named(n) => {
                let expr = unwrap_expr(n.expr()).to_untyped().clone().into_text();
                add_param(ParamSpec {
                    name: &n.name().get().into(),
                    docs: Some(&eco_format!("Default value: {expr}")),
                    default: Some(&expr),
                    ty: &Ty::Any,
                    attrs: ParamAttrs {
                        positional: false,
                        named: true,
                        variadic: false,
                        settable: true,
                    },
                });
            }
            ast::Param::Spread(n) => {
                let ident = n.sink_ident().map(|e| e.as_str());
                add_param(ParamSpec {
                    name: &ident.unwrap_or_default().into(),
                    docs: None,
                    default: None,
                    ty: &Ty::Any,
                    attrs: ParamAttrs {
                        positional: true,
                        named: false,
                        variadic: true,
                        settable: false,
                    },
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
