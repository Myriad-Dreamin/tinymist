use log::debug;
use typst::syntax::{
    ast::{self, AstNode},
    LinkedNode, SyntaxKind,
};

pub fn deref_lvalue(mut node: LinkedNode) -> Option<LinkedNode> {
    while let Some(e) = node.cast::<ast::Parenthesized>() {
        node = node.find(e.expr().span())?;
    }
    Some(node)
}

#[derive(Debug, Clone)]
pub enum DerefTarget<'a> {
    VarAccess(LinkedNode<'a>),
    Callee(LinkedNode<'a>),
    ImportPath(LinkedNode<'a>),
}

impl<'a> DerefTarget<'a> {
    pub fn node(&self) -> &LinkedNode {
        match self {
            DerefTarget::VarAccess(node) => node,
            DerefTarget::Callee(node) => node,
            DerefTarget::ImportPath(node) => node,
        }
    }
}

fn is_mark(sk: SyntaxKind) -> bool {
    use SyntaxKind::*;
    matches!(
        sk,
        MathAlignPoint
            | Plus
            | Minus
            | Slash
            | Hat
            | Dot
            | Eq
            | EqEq
            | ExclEq
            | Lt
            | LtEq
            | Gt
            | GtEq
            | PlusEq
            | HyphEq
            | StarEq
            | SlashEq
            | Dots
            | Arrow
            | Not
            | And
            | Or
            | LeftBrace
            | RightBrace
            | LeftBracket
            | RightBracket
            | LeftParen
            | RightParen
            | Comma
            | Semicolon
            | Colon
            | Hash
    )
}

pub fn get_deref_target(node: LinkedNode) -> Option<DerefTarget> {
    let mut ancestor = node;
    if ancestor.kind().is_trivia() || is_mark(ancestor.kind()) {
        ancestor = ancestor.prev_sibling()?;
    }

    while !ancestor.is::<ast::Expr>() {
        ancestor = ancestor.parent()?.clone();
    }
    debug!("deref expr: {ancestor:?}");
    let ancestor = deref_lvalue(ancestor)?;
    debug!("deref lvalue: {ancestor:?}");

    let may_ident = ancestor.cast::<ast::Expr>()?;
    if !may_ident.hash() && !matches!(may_ident, ast::Expr::MathIdent(_)) {
        return None;
    }

    Some(match may_ident {
        // todo: label, reference
        // todo: import
        // todo: include
        ast::Expr::FuncCall(call) => DerefTarget::Callee(ancestor.find(call.callee().span())?),
        ast::Expr::Set(set) => DerefTarget::Callee(ancestor.find(set.target().span())?),
        ast::Expr::Ident(..) | ast::Expr::MathIdent(..) | ast::Expr::FieldAccess(..) => {
            DerefTarget::VarAccess(ancestor.find(may_ident.span())?)
        }
        ast::Expr::Str(..) => {
            let parent = ancestor.parent()?;
            if parent.kind() != SyntaxKind::ModuleImport {
                return None;
            }

            return Some(DerefTarget::ImportPath(ancestor.find(may_ident.span())?));
        }
        ast::Expr::Import(..) => {
            return None;
        }
        _ => {
            debug!("unsupported kind {kind:?}", kind = ancestor.kind());
            return None;
        }
    })
}
