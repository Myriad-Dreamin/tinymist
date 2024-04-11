//! Linked definition analysis

use std::ops::Range;

use log::debug;
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

    // syntactic definition
    let def_use = ctx.def_use(source)?;
    let ident_ref = match use_site.cast::<ast::Expr>()? {
        ast::Expr::Ident(e) => IdentRef {
            name: e.get().to_string(),
            range: use_site.range(),
        },
        ast::Expr::MathIdent(e) => IdentRef {
            name: e.get().to_string(),
            range: use_site.range(),
        },
        ast::Expr::FieldAccess(..) => {
            debug!("find field access");
            return None;
        }
        _ => {
            debug!("unsupported kind {kind:?}", kind = use_site.kind());
            return None;
        }
    };
    let def_id = def_use.get_ref(&ident_ref);
    let def_id = def_id.or_else(|| Some(def_use.get_def(source_id, &ident_ref)?.0));
    let def_info = def_id.and_then(|def_id| def_use.get_def_by_id(def_id));

    let values = analyze_expr(ctx.world(), &use_site);
    for v in values {
        // mostly builtin functions
        if let Value::Func(f) = v.0 {
            use typst::foundations::func::Repr;
            match f.inner() {
                // The with function should be resolved as the with position
                Repr::Closure(..) | Repr::With(..) => continue,
                Repr::Native(..) | Repr::Element(..) => {}
            }

            let name = f
                .name()
                .or_else(|| def_info.as_ref().map(|(_, r)| r.name.as_str()));

            if let Some(name) = name {
                let span = f.span();
                let fid = span.id()?;
                let source = ctx.source_by_id(fid).ok()?;

                return Some(DefinitionLink {
                    kind: LexicalKind::Var(LexicalVarKind::Function),
                    name: name.to_owned(),
                    value: Some(Value::Func(f.clone())),
                    // value: None,
                    def_at: Some((fid, source.find(span)?.range())),
                    name_range: def_info.map(|(_, r)| r.range.clone()),
                });
            }
        }
    }

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

/// Resolve a callee expression to a function.
pub fn resolve_callee(ctx: &mut AnalysisContext, callee: LinkedNode) -> Option<Func> {
    {
        let values = analyze_expr(ctx.world(), &callee);

        values.into_iter().find_map(|v| match v.0 {
            Value::Func(f) => Some(f),
            _ => None,
        })
    }
    .or_else(|| {
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
        resolve_global_value(ctx, callee, false).and_then(|v| match v {
            Value::Func(f) => Some(f),
            _ => None,
        })
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
