use log::debug;
use typst::{
    foundations::{Func, ParamInfo},
    syntax::{
        ast::{self, AstNode},
        LinkedNode, SyntaxKind,
    },
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
    IncludePath(LinkedNode<'a>),
}

impl<'a> DerefTarget<'a> {
    pub fn node(&self) -> &LinkedNode {
        match self {
            DerefTarget::VarAccess(node) => node,
            DerefTarget::Callee(node) => node,
            DerefTarget::ImportPath(node) => node,
            DerefTarget::IncludePath(node) => node,
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

pub fn get_deref_target(node: LinkedNode, cursor: usize) -> Option<DerefTarget> {
    fn same_line_skip(node: &LinkedNode, cursor: usize) -> bool {
        // (ancestor.kind().is_trivia() && ancestor.text())
        if !node.kind().is_trivia() {
            return false;
        }
        let pref = node.text();
        // slice
        let pref = if cursor < pref.len() {
            &pref[..cursor]
        } else {
            pref
        };
        // no newlines
        // todo: if we are in markup mode, we should check if we are at start of node
        !pref.contains('\n')
    }

    let mut ancestor = node;
    if same_line_skip(&ancestor, cursor) || is_mark(ancestor.kind()) {
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
        ast::Expr::FuncCall(call) => DerefTarget::Callee(ancestor.find(call.callee().span())?),
        ast::Expr::Set(set) => DerefTarget::Callee(ancestor.find(set.target().span())?),
        ast::Expr::Ident(..) | ast::Expr::MathIdent(..) | ast::Expr::FieldAccess(..) => {
            DerefTarget::VarAccess(ancestor.find(may_ident.span())?)
        }
        ast::Expr::Str(..) => {
            let parent = ancestor.parent()?;
            if parent.kind() == SyntaxKind::ModuleImport {
                return Some(DerefTarget::ImportPath(ancestor.find(may_ident.span())?));
            }
            if parent.kind() == SyntaxKind::ModuleInclude {
                return Some(DerefTarget::IncludePath(ancestor.find(may_ident.span())?));
            }

            return None;
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

#[derive(Debug, Clone)]
pub enum DefTarget<'a> {
    Let(LinkedNode<'a>),
    Import(LinkedNode<'a>),
}

impl<'a> DefTarget<'a> {
    pub fn node(&self) -> &LinkedNode {
        match self {
            DefTarget::Let(node) => node,
            DefTarget::Import(node) => node,
        }
    }
}

pub fn get_def_target(node: LinkedNode) -> Option<DefTarget<'_>> {
    let mut ancestor = node;
    if ancestor.kind().is_trivia() || is_mark(ancestor.kind()) {
        ancestor = ancestor.prev_sibling()?;
    }

    while !ancestor.is::<ast::Expr>() {
        ancestor = ancestor.parent()?.clone();
    }
    debug!("def expr: {ancestor:?}");
    let ancestor = deref_lvalue(ancestor)?;
    debug!("def lvalue: {ancestor:?}");

    let may_ident = ancestor.cast::<ast::Expr>()?;
    if !may_ident.hash() && !matches!(may_ident, ast::Expr::MathIdent(_)) {
        return None;
    }

    Some(match may_ident {
        // todo: label, reference
        // todo: import
        // todo: include
        ast::Expr::FuncCall(..) => return None,
        ast::Expr::Set(..) => return None,
        ast::Expr::Let(..) => DefTarget::Let(ancestor),
        ast::Expr::Import(..) => DefTarget::Import(ancestor),
        // todo: parameter
        ast::Expr::Ident(..)
        | ast::Expr::MathIdent(..)
        | ast::Expr::FieldAccess(..)
        | ast::Expr::Closure(..) => {
            let mut ancestor = ancestor;
            while !ancestor.is::<ast::LetBinding>() {
                ancestor = ancestor.parent()?.clone();
            }

            DefTarget::Let(ancestor)
        }
        ast::Expr::Str(..) => {
            let parent = ancestor.parent()?;
            if parent.kind() != SyntaxKind::ModuleImport {
                return None;
            }

            DefTarget::Import(parent.clone())
        }
        _ => {
            debug!("unsupported kind {kind:?}", kind = ancestor.kind());
            return None;
        }
    })
}

pub fn param_index_at_leaf(leaf: &LinkedNode, function: &Func, args: ast::Args) -> Option<usize> {
    let deciding = deciding_syntax(leaf);
    let params = function.params()?;
    let param_index = find_param_index(&deciding, params, args)?;
    log::trace!("got param index {param_index}");
    Some(param_index)
}

/// Find the piece of syntax that decides what we're completing.
fn deciding_syntax<'b>(leaf: &'b LinkedNode) -> LinkedNode<'b> {
    let mut deciding = leaf.clone();
    while !matches!(
        deciding.kind(),
        SyntaxKind::LeftParen | SyntaxKind::Comma | SyntaxKind::Colon
    ) {
        let Some(prev) = deciding.prev_leaf() else {
            break;
        };
        deciding = prev;
    }
    deciding
}

fn find_param_index(deciding: &LinkedNode, params: &[ParamInfo], args: ast::Args) -> Option<usize> {
    match deciding.kind() {
        // After colon: "func(param:|)", "func(param: |)".
        SyntaxKind::Colon => {
            let prev = deciding.prev_leaf()?;
            let param_ident = prev.cast::<ast::Ident>()?;
            params
                .iter()
                .position(|param| param.name == param_ident.as_str())
        }
        // Before: "func(|)", "func(hi|)", "func(12,|)".
        SyntaxKind::Comma | SyntaxKind::LeftParen => {
            let next = deciding.next_leaf();
            let following_param = next.as_ref().and_then(|next| next.cast::<ast::Ident>());
            match following_param {
                Some(next) => params
                    .iter()
                    .position(|param| param.named && param.name.starts_with(next.as_str())),
                None => {
                    let positional_args_so_far = args
                        .items()
                        .filter(|arg| matches!(arg, ast::Arg::Pos(_)))
                        .count();
                    params
                        .iter()
                        .enumerate()
                        .filter(|(_, param)| param.positional)
                        .map(|(i, _)| i)
                        .nth(positional_args_so_far)
                }
            }
        }
        _ => None,
    }
}
