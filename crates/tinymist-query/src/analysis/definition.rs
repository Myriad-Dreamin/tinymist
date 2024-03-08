use log::trace;
use typst::{
    foundations::{Func, Value},
    syntax::{
        ast::{self, AstNode},
        LinkedNode,
    },
};
use typst_ts_compiler::TypstSystemWorld;

use crate::{prelude::analyze_expr, TypstSpan};

#[derive(Debug, Clone)]
pub struct FuncDefinition<'a> {
    pub value: Func,
    pub use_site: LinkedNode<'a>,
    pub span: TypstSpan,
}

#[derive(Debug, Clone)]
pub enum Definition<'a> {
    Func(FuncDefinition<'a>),
}

// todo: field definition
pub(crate) fn find_definition<'a>(
    world: &TypstSystemWorld,
    node: LinkedNode<'a>,
) -> Option<Definition<'a>> {
    let mut ancestor = &node;
    while !ancestor.is::<ast::Expr>() {
        ancestor = ancestor.parent()?;
    }

    let may_ident = ancestor.cast::<ast::Expr>()?;
    if !may_ident.hash() && !matches!(may_ident, ast::Expr::MathIdent(_)) {
        return None;
    }

    let mut is_ident_only = false;
    trace!("got ast_node kind {kind:?}", kind = ancestor.kind());
    let callee_node = match may_ident {
        // todo: label, reference
        // todo: import
        // todo: include
        ast::Expr::FuncCall(call) => call.callee(),
        ast::Expr::Set(set) => set.target(),
        ast::Expr::Ident(..) | ast::Expr::MathIdent(..) | ast::Expr::FieldAccess(..) => {
            is_ident_only = true;
            may_ident
        }
        _ => return None,
    };
    trace!("got callee_node {callee_node:?} {is_ident_only:?}");

    let use_site = if is_ident_only {
        ancestor.clone()
    } else {
        ancestor.find(callee_node.span())?
    };

    let values = analyze_expr(world, &use_site);

    let func_or_module = values.into_iter().find_map(|v| match &v {
        Value::Args(a) => {
            trace!("got args {a:?}");
            None
        }
        Value::Func(..) | Value::Module(..) => Some(v),
        _ => None,
    });

    Some(match func_or_module {
        Some(Value::Func(f)) => Definition::Func(FuncDefinition {
            value: f.clone(),
            span: f.span(),
            use_site,
        }),
        Some(Value::Module(m)) => {
            trace!("find module. {m:?}");
            // todo
            return None;
        }
        _ => {
            trace!("find value by lexical result. {use_site:?}");
            return None;
        }
    })
}
