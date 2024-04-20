//! Linked definition analysis

use std::ops::Range;

use log::debug;
use once_cell::sync::Lazy;
use typst::foundations::Type;
use typst::syntax::FileId as TypstFileId;
use typst::{foundations::Value, syntax::Span};

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
    pub name: String,
    /// The location of the definition.
    pub def_at: Option<(TypstFileId, Range<usize>)>,
    /// The range of the name of the definition.
    pub name_range: Option<Range<usize>>,
}

// todo: field definition
/// Finds the definition of a symbol.
pub fn find_definition(
    ctx: &mut AnalysisContext<'_>,
    source: Source,
    deref_target: DerefTarget<'_>,
) -> Option<DefinitionLink> {
    let source_id = source.id();

    let use_site = match deref_target {
        // todi: field access
        DerefTarget::VarAccess(node) | DerefTarget::Callee(node) => node,
        // todo: better support (rename import path?)
        DerefTarget::ImportPath(path) => {
            let parent = path.parent()?;
            let def_fid = parent.span().id()?;
            let import_node = parent.cast::<ast::ModuleImport>()?;
            let source = find_source_by_expr(ctx.world(), def_fid, import_node.source())?;
            return Some(DefinitionLink {
                kind: LexicalKind::Mod(LexicalModKind::PathVar),
                name: String::new(),
                value: None,
                def_at: Some((source.id(), LinkedNode::new(source.root()).range())),
                name_range: None,
            });
        }
        DerefTarget::IncludePath(path) => {
            let parent = path.parent()?;
            let def_fid = parent.span().id()?;
            let include_node = parent.cast::<ast::ModuleInclude>()?;
            let source = find_source_by_expr(ctx.world(), def_fid, include_node.source())?;
            return Some(DefinitionLink {
                kind: LexicalKind::Mod(LexicalModKind::PathInclude),
                name: String::new(),
                value: None,
                def_at: Some((source.id(), (LinkedNode::new(source.root())).range())),
                name_range: None,
            });
        }
        // todo: label, reference
        DerefTarget::Label(..) | DerefTarget::Ref(..) | DerefTarget::Normal(..) => {
            return None;
        }
    };

    // Lexical reference
    let ident_ref = match use_site.cast::<ast::Expr>()? {
        ast::Expr::Ident(e) => Some(IdentRef {
            name: e.get().to_string(),
            range: use_site.range(),
        }),
        ast::Expr::MathIdent(e) => Some(IdentRef {
            name: e.get().to_string(),
            range: use_site.range(),
        }),
        ast::Expr::FieldAccess(..) => {
            debug!("find field access");

            None
        }
        _ => {
            debug!("unsupported kind {kind:?}", kind = use_site.kind());
            None
        }
    };

    // Syntactic definition
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
            value_to_def(
                ctx,
                f,
                || Some(use_site.get().clone().into_text().to_string()),
                None,
            )
        });
    };

    match def.kind {
        LexicalKind::Heading(..) | LexicalKind::Block => unreachable!(),
        LexicalKind::Var(
            LexicalVarKind::Variable
            | LexicalVarKind::ValRef
            | LexicalVarKind::Label
            | LexicalVarKind::LabelRef,
        )
        | LexicalKind::Mod(
            LexicalModKind::Module(..)
            | LexicalModKind::PathVar
            | LexicalModKind::PathInclude
            | LexicalModKind::ModuleAlias
            | LexicalModKind::Alias { .. }
            | LexicalModKind::Ident,
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
            let def_name = root.leaf_at(def.range.start + 1)?;
            log::info!("def_name for function: {def_name:?}", def_name = def_name);
            let values = analyze_expr(ctx.world(), &def_name);
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
            log::info!("unimplemented star import {:?}", ident_ref);
            None
        }
    }
}

/// The target of a dynamic call.
#[derive(Debug, Clone)]
pub struct DynCallTarget {
    /// The function pointer.
    pub func_ptr: Func,
    /// The this pointer.
    pub this: Option<Value>,
}

/// The calling convention of a function.
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
    callee: LinkedNode,
) -> Option<CallConvention> {
    resolve_callee_(ctx, callee, true).map(identify_call_convention)
}

/// Resolve a callee expression to a function.
pub fn resolve_callee(ctx: &mut AnalysisContext, callee: LinkedNode) -> Option<Func> {
    resolve_callee_(ctx, callee, false).map(|e| e.func_ptr)
}

fn resolve_callee_(
    ctx: &mut AnalysisContext,
    callee: LinkedNode,
    resolve_this: bool,
) -> Option<DynCallTarget> {
    None.or_else(|| {
        let source = ctx.source_by_id(callee.span().id()?).ok()?;
        let node = source.find(callee.span())?;
        let cursor = node.offset();
        let deref_target = get_deref_target(node, cursor)?;
        let def = find_definition(ctx, source.clone(), deref_target)?;
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
        let values = analyze_expr(ctx.world(), &callee);

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
                let values = analyze_expr(ctx.world(), &callee.find(target.span())?);
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
    name: impl FnOnce() -> Option<String>,
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
            let name = func.name().map(|e| e.to_owned()).or_else(name)?;
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
            let name = module.name().to_string();
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
