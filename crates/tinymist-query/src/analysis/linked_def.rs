//! Linked definition analysis

use std::ops::Range;

use log::debug;
use once_cell::sync::Lazy;
use typst::foundations::{IntoValue, Label, Selector, Type};
use typst::introspection::Introspector;
use typst::model::BibliographyElem;
use typst::syntax::FileId as TypstFileId;
use typst::{foundations::Value, syntax::Span};
use typst_shim::syntax::LinkedNodeExt;

use super::prelude::*;
use crate::{
    prelude::*,
    syntax::{
        find_source_by_expr, get_deref_target, DerefTarget, IdentRef, LexicalKind, LexicalModKind,
        LexicalVarKind,
    },
};

/// A linked definition in the source code
pub struct DefinitionLink {
    /// The kind of the definition.
    pub kind: LexicalKind,
    /// A possible instance of the definition.
    pub value: Option<Value>,
    /// The name of the definition.
    pub name: EcoString,
    /// The location of the definition.
    pub def_at: Option<(TypstFileId, Range<usize>)>,
    /// The range of the name of the definition.
    pub name_range: Option<Range<usize>>,
}

impl DefinitionLink {
    /// Convert the definition to an identifier reference.
    pub fn to_ident_ref(&self) -> Option<IdentRef> {
        Some(IdentRef {
            name: self.name.clone(),
            range: self.name_range.clone()?,
        })
    }
}

// todo: field definition
/// Finds the definition of a symbol.
pub fn find_definition(
    ctx: &mut AnalysisContext<'_>,
    source: Source,
    document: Option<&VersionedDocument>,
    deref_target: DerefTarget<'_>,
) -> Option<DefinitionLink> {
    match deref_target {
        // todi: field access
        DerefTarget::VarAccess(node) | DerefTarget::Callee(node) => {
            find_ident_definition(ctx, source, node)
        }
        // todo: better support (rename import path?)
        DerefTarget::ImportPath(path) => {
            let parent = path.parent()?;
            let def_fid = parent.span().id()?;
            let import_node = parent.cast::<ast::ModuleImport>()?;
            let source = find_source_by_expr(ctx.world(), def_fid, import_node.source())?;
            Some(DefinitionLink {
                kind: LexicalKind::Mod(LexicalModKind::PathVar),
                name: EcoString::new(),
                value: None,
                def_at: Some((source.id(), LinkedNode::new(source.root()).range())),
                name_range: None,
            })
        }
        DerefTarget::IncludePath(path) => {
            let parent = path.parent()?;
            let def_fid = parent.span().id()?;
            let include_node = parent.cast::<ast::ModuleInclude>()?;
            let source = find_source_by_expr(ctx.world(), def_fid, include_node.source())?;
            Some(DefinitionLink {
                kind: LexicalKind::Mod(LexicalModKind::PathInclude),
                name: EcoString::new(),
                value: None,
                def_at: Some((source.id(), (LinkedNode::new(source.root())).range())),
                name_range: None,
            })
        }
        DerefTarget::Label(r) | DerefTarget::Ref(r) => {
            let ref_expr: ast::Expr = r.cast()?;
            let (ref_node, is_label) = match ref_expr {
                ast::Expr::Ref(r) => (r.target(), false),
                ast::Expr::Label(r) => (r.get(), true),
                _ => return None,
            };

            let introspector = &document?.document.introspector;
            find_bib_definition(ctx, introspector, ref_node)
                .or_else(|| find_ref_definition(ctx, introspector, ref_node, is_label, r.span()))
        }
        DerefTarget::Normal(..) => None,
    }
}

fn find_ident_definition(
    ctx: &mut AnalysisContext<'_>,
    source: Source,
    mut use_site: LinkedNode,
) -> Option<DefinitionLink> {
    let mut proj = vec![];
    // Lexical reference
    let ident_store = use_site.clone();
    let ident_ref = match ident_store.cast::<ast::Expr>()? {
        ast::Expr::Ident(e) => Some(IdentRef {
            name: e.get().clone(),
            range: use_site.range(),
        }),
        ast::Expr::MathIdent(e) => Some(IdentRef {
            name: e.get().clone(),
            range: use_site.range(),
        }),
        ast::Expr::FieldAccess(s) => {
            proj.push(s.field());

            let mut i = s.target();
            while let ast::Expr::FieldAccess(f) = i {
                proj.push(f.field());
                i = f.target();
            }

            match i {
                ast::Expr::Ident(e) => {
                    use_site = use_site.find(e.span())?;
                    Some(IdentRef {
                        name: e.get().clone(),
                        range: use_site.range(),
                    })
                }
                ast::Expr::MathIdent(e) => {
                    use_site = use_site.find(e.span())?;
                    Some(IdentRef {
                        name: e.get().clone(),
                        range: use_site.range(),
                    })
                }
                _ => None,
            }
        }
        _ => {
            debug!("unsupported kind {kind:?}", kind = use_site.kind());
            None
        }
    };

    // Syntactic definition
    let source_id = source.id();
    let def_use = ctx.def_use(source);
    let def_info = ident_ref
        .as_ref()
        .zip(def_use.as_ref())
        .and_then(|(ident_ref, def_use)| {
            let def_id = def_use.get_ref(ident_ref);
            let def_id = def_id.or_else(|| Some(def_use.get_def(source_id, ident_ref)?.0))?;

            def_use.get_def_by_id(def_id)
        });

    // Global definition
    let Some((def_fid, def)) = def_info else {
        return resolve_global_value(ctx, use_site.clone(), false).and_then(move |f| {
            value_to_def(ctx, f, || Some(use_site.get().clone().into_text()), None)
        });
    };

    match &def.kind {
        LexicalKind::Var(LexicalVarKind::BibKey)
        | LexicalKind::Heading(..)
        | LexicalKind::Block => unreachable!(),
        LexicalKind::Mod(
            LexicalModKind::Module(..) | LexicalModKind::PathVar | LexicalModKind::ModuleAlias,
        ) => {
            if !proj.is_empty() {
                proj.reverse();
                let m = ctx.module_ins_at(def_fid, def.range.start + 1)?;
                let val = project_value(&m, proj.as_slice())?;

                // todo: name range
                let name = proj.last().map(|e| e.get().clone());
                return value_to_def(ctx, val.clone(), || name, None);
            }

            Some(DefinitionLink {
                kind: def.kind.clone(),
                name: def.name.clone(),
                value: None,
                def_at: Some((def_fid, def.range.clone())),
                name_range: Some(def.range.clone()),
            })
        }
        LexicalKind::Var(
            LexicalVarKind::Variable
            | LexicalVarKind::ValRef
            | LexicalVarKind::Label
            | LexicalVarKind::LabelRef,
        )
        | LexicalKind::Mod(
            LexicalModKind::PathInclude | LexicalModKind::Alias { .. } | LexicalModKind::Ident,
        ) => Some(DefinitionLink {
            kind: def.kind.clone(),
            name: def.name.clone(),
            value: None,
            def_at: Some((def_fid, def.range.clone())),
            name_range: Some(def.range.clone()),
        }),
        LexicalKind::Var(LexicalVarKind::Function) => {
            let def_source = ctx.source_by_id(def_fid).ok()?;
            let root = LinkedNode::new(def_source.root());
            let def_name = root.leaf_at_compat(def.range.start + 1)?;
            log::info!("def_name for function: {def_name:?}", def_name = def_name);
            let values = ctx.analyze_expr(&def_name);
            let func = values.into_iter().find(|v| matches!(v.0, Value::Func(..)));
            log::info!("okay for function: {func:?}");

            Some(DefinitionLink {
                kind: def.kind.clone(),
                name: def.name.clone(),
                value: func.map(|v| v.0),
                // value: None,
                def_at: Some((def_fid, def.range.clone())),
                name_range: Some(def.range.clone()),
            })
        }
        LexicalKind::Mod(LexicalModKind::Star) => {
            log::info!("unimplemented star import {ident_ref:?}");
            None
        }
    }
}

fn project_value<'a>(m: &'a Value, proj: &[ast::Ident<'_>]) -> Option<&'a Value> {
    if proj.is_empty() {
        return Some(m);
    }
    let scope = m.scope()?;
    let (ident, proj) = proj.split_first()?;
    let v = scope.get(ident.as_str())?;
    project_value(v, proj)
}

fn find_bib_definition(
    ctx: &mut AnalysisContext,
    introspector: &Introspector,
    key: &str,
) -> Option<DefinitionLink> {
    let bib_elem = BibliographyElem::find(introspector.track()).ok()?;
    let Value::Array(arr) = bib_elem.path().clone().into_value() else {
        return None;
    };

    let bib_paths = arr.into_iter().map(Value::cast).flat_map(|e| e.ok());
    let bib_info = ctx.analyze_bib(bib_elem.span(), bib_paths)?;

    let entry = bib_info.entries.get(key);
    log::debug!("find_bib_definition: {key} => {entry:?}");
    let entry = entry?;
    Some(DefinitionLink {
        kind: LexicalKind::Var(LexicalVarKind::BibKey),
        name: key.into(),
        value: None,
        def_at: Some((entry.file_id, entry.span.clone())),
        // todo: rename with regard to string format: yaml-key/bib etc.
        name_range: Some(entry.span.clone()),
    })
}

fn find_ref_definition(
    ctx: &mut AnalysisContext,
    introspector: &Introspector,
    ref_node: &str,
    is_label: bool,
    span: Span,
) -> Option<DefinitionLink> {
    let label = Label::new(ref_node);
    let sel = Selector::Label(label);
    let elem = introspector.query_first(&sel)?;

    // if it is a label, we put the selection range to itself
    let (def_at, name_range) = if is_label {
        let fid = span.id()?;
        let source = ctx.source_by_id(fid).ok()?;
        let rng = source.range(span)?;

        let name_range = rng.start + 1..rng.end - 1;
        let name_range = (name_range.start <= name_range.end).then_some(name_range);
        (Some((fid, rng)), name_range)
    } else {
        let span = elem.labelled_at();
        let span = if !span.is_detached() {
            span
        } else {
            // otherwise, it is estimated to the span of the pointed content
            elem.span()
        };
        let fid = span.id()?;
        let source = ctx.source_by_id(fid).ok()?;
        let rng = source.range(span)?;

        (Some((fid, rng)), None)
    };

    Some(DefinitionLink {
        kind: LexicalKind::Var(LexicalVarKind::Label),
        name: ref_node.into(),
        value: Some(Value::Content(elem)),
        def_at,
        name_range,
    })
}

/// The target of a dynamic call.
#[derive(Debug, Clone)]
pub struct DynCallTarget {
    /// The function pointer.
    pub func_ptr: Func,
    /// The this pointer.
    pub this: Option<Value>,
}

/// The call of a function with calling convention identified.
#[derive(Debug, Clone)]
pub enum CallConvention {
    /// A static function.
    Static(Func),
    /// A method call with a this.
    Method(Value, Func),
    /// A function call by with binding.
    With(Func),
    /// A function call by where binding.
    Where(Func),
}

impl CallConvention {
    /// Get the function pointer of the call.
    pub fn method_this(&self) -> Option<&Value> {
        match self {
            CallConvention::Static(_) => None,
            CallConvention::Method(this, _) => Some(this),
            CallConvention::With(_) => None,
            CallConvention::Where(_) => None,
        }
    }

    /// Get the function pointer of the call.
    pub fn callee(self) -> Func {
        match self {
            CallConvention::Static(f) => f,
            CallConvention::Method(_, f) => f,
            CallConvention::With(f) => f,
            CallConvention::Where(f) => f,
        }
    }
}

fn identify_call_convention(target: DynCallTarget) -> CallConvention {
    match target.this {
        Some(Value::Func(func)) if is_with_func(&target.func_ptr) => CallConvention::With(func),
        Some(Value::Func(func)) if is_where_func(&target.func_ptr) => CallConvention::Where(func),
        Some(this) => CallConvention::Method(this, target.func_ptr),
        None => CallConvention::Static(target.func_ptr),
    }
}

fn is_with_func(func_ptr: &Func) -> bool {
    static WITH_FUNC: Lazy<Option<&'static Func>> = Lazy::new(|| {
        let fn_ty = Type::of::<Func>();
        let Some(Value::Func(f)) = fn_ty.scope().get("with") else {
            return None;
        };
        Some(f)
    });

    is_same_native_func(*WITH_FUNC, func_ptr)
}

fn is_where_func(func_ptr: &Func) -> bool {
    static WITH_FUNC: Lazy<Option<&'static Func>> = Lazy::new(|| {
        let fn_ty = Type::of::<Func>();
        let Some(Value::Func(f)) = fn_ty.scope().get("where") else {
            return None;
        };
        Some(f)
    });

    is_same_native_func(*WITH_FUNC, func_ptr)
}

fn is_same_native_func(x: Option<&Func>, y: &Func) -> bool {
    let Some(x) = x else {
        return false;
    };

    use typst::foundations::func::Repr;
    match (x.inner(), y.inner()) {
        (Repr::Native(x), Repr::Native(y)) => x == y,
        (Repr::Element(x), Repr::Element(y)) => x == y,
        _ => false,
    }
}

// todo: merge me with resolve_callee
/// Resolve a call target to a function or a method with a this.
pub fn resolve_call_target(
    ctx: &mut AnalysisContext,
    callee: &LinkedNode,
) -> Option<CallConvention> {
    resolve_callee_(ctx, callee, true).map(identify_call_convention)
}

/// Resolve a callee expression to a function.
pub fn resolve_callee(ctx: &mut AnalysisContext, callee: &LinkedNode) -> Option<Func> {
    resolve_callee_(ctx, callee, false).map(|e| e.func_ptr)
}

fn resolve_callee_(
    ctx: &mut AnalysisContext,
    callee: &LinkedNode,
    resolve_this: bool,
) -> Option<DynCallTarget> {
    None.or_else(|| {
        let source = ctx.source_by_id(callee.span().id()?).ok()?;
        let node = source.find(callee.span())?;
        let cursor = node.offset();
        let deref_target = get_deref_target(node, cursor)?;
        let def = find_definition(ctx, source.clone(), None, deref_target)?;
        match def.kind {
            LexicalKind::Var(LexicalVarKind::Function) => match def.value {
                Some(Value::Func(f)) => Some(f),
                _ => None,
            },
            _ => None,
        }
    })
    .or_else(|| {
        resolve_global_value(ctx, callee.clone(), false).and_then(|v| match v {
            Value::Func(f) => Some(f),
            _ => None,
        })
    })
    .map(|e| DynCallTarget {
        func_ptr: e,
        this: None,
    })
    .or_else(|| {
        let values = ctx.analyze_expr(callee);

        if let Some(func) = values.into_iter().find_map(|v| match v.0 {
            Value::Func(f) => Some(f),
            _ => None,
        }) {
            return Some(DynCallTarget {
                func_ptr: func,
                this: None,
            });
        };

        if resolve_this {
            if let Some(access) = match callee.cast::<ast::Expr>() {
                Some(ast::Expr::FieldAccess(access)) => Some(access),
                _ => None,
            } {
                let target = access.target();
                let field = access.field().get();
                let values = ctx.analyze_expr(&callee.find(target.span())?);
                if let Some((this, func_ptr)) = values.into_iter().find_map(|(this, _styles)| {
                    if let Some(Value::Func(f)) = this.ty().scope().get(field) {
                        return Some((this, f.clone()));
                    }

                    None
                }) {
                    return Some(DynCallTarget {
                        func_ptr,
                        this: Some(this),
                    });
                }
            }
        }

        None
    })
}

// todo: math scope
pub(crate) fn resolve_global_value(
    ctx: &AnalysisContext,
    callee: LinkedNode,
    is_math: bool,
) -> Option<Value> {
    let lib = ctx.world().library();
    let scope = if is_math {
        lib.math.scope()
    } else {
        lib.global.scope()
    };
    let v = match callee.cast::<ast::Expr>()? {
        ast::Expr::Ident(ident) => scope.get(&ident)?,
        ast::Expr::FieldAccess(access) => match access.target() {
            ast::Expr::Ident(target) => match scope.get(&target)? {
                Value::Module(module) => module.field(&access.field()).ok()?,
                Value::Func(func) => func.field(&access.field()).ok()?,
                _ => return None,
            },
            _ => return None,
        },
        _ => return None,
    };
    Some(v.clone())
}

fn value_to_def(
    ctx: &mut AnalysisContext,
    value: Value,
    name: impl FnOnce() -> Option<EcoString>,
    name_range: Option<Range<usize>>,
) -> Option<DefinitionLink> {
    let mut def_at = |span: Span| {
        span.id().and_then(|fid| {
            let source = ctx.source_by_id(fid).ok()?;
            Some((fid, source.find(span)?.range()))
        })
    };

    Some(match value {
        Value::Func(func) => {
            let name = func.name().map(|e| e.into()).or_else(name)?;
            let span = func.span();
            DefinitionLink {
                kind: LexicalKind::Var(LexicalVarKind::Function),
                name,
                value: Some(Value::Func(func)),
                def_at: def_at(span),
                name_range,
            }
        }
        Value::Module(module) => {
            let name = module.name().clone();
            DefinitionLink {
                kind: LexicalKind::Var(LexicalVarKind::Variable),
                name,
                value: None,
                def_at: None,
                name_range,
            }
        }
        _v => {
            let name = name()?;
            DefinitionLink {
                kind: LexicalKind::Mod(LexicalModKind::PathVar),
                name,
                value: None,
                def_at: None,
                name_range,
            }
        }
    })
}
