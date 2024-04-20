use ecow::EcoVec;
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

#[derive(Debug, Clone)]
pub enum DerefTarget<'a> {
    Label(LinkedNode<'a>),
    Ref(LinkedNode<'a>),
    VarAccess(LinkedNode<'a>),
    Callee(LinkedNode<'a>),
    ImportPath(LinkedNode<'a>),
    IncludePath(LinkedNode<'a>),
    Normal(SyntaxKind, LinkedNode<'a>),
}

impl<'a> DerefTarget<'a> {
    pub fn node(&self) -> &LinkedNode<'a> {
        match self {
            DerefTarget::Label(node) => node,
            DerefTarget::Ref(node) => node,
            DerefTarget::VarAccess(node) => node,
            DerefTarget::Callee(node) => node,
            DerefTarget::ImportPath(node) => node,
            DerefTarget::IncludePath(node) => node,
            DerefTarget::Normal(_, node) => node,
        }
    }
}

pub fn get_deref_target(node: LinkedNode, cursor: usize) -> Option<DerefTarget<'_>> {
    /// Skips trivia nodes that are on the same line as the cursor.
    fn skippable_trivia(node: &LinkedNode, cursor: usize) -> bool {
        // A non-trivia node is our target so we stop at it.
        if !node.kind().is_trivia() {
            return false;
        }

        // Get the trivia text before the cursor.
        let pref = node.text();
        let pref = if node.range().contains(&cursor) {
            &pref[..cursor - node.offset()]
        } else {
            pref
        };

        // The deref target should be on the same line as the cursor.
        // todo: if we are in markup mode, we should check if we are at start of node
        !pref.contains('\n')
    }

    // Move to the first non-trivia node before the cursor.
    let mut node = node;
    if skippable_trivia(&node, cursor) {
        node = node.prev_sibling()?;
    }

    // Move to the first ancestor that is an expression.
    let mut ancestor = node;
    while !ancestor.is::<ast::Expr>() {
        ancestor = ancestor.parent()?.clone();
    }
    debug!("deref expr: {ancestor:?}");

    // Unwrap all parentheses to get the actual expression.
    let cano_expr = deref_lvalue(ancestor)?;
    debug!("deref lvalue: {cano_expr:?}");

    // Identify convenient expression kinds.
    let expr = cano_expr.cast::<ast::Expr>()?;
    Some(match expr {
        ast::Expr::Label(..) => DerefTarget::Label(cano_expr),
        ast::Expr::Ref(..) => DerefTarget::Ref(cano_expr),
        ast::Expr::FuncCall(call) => DerefTarget::Callee(cano_expr.find(call.callee().span())?),
        ast::Expr::Set(set) => DerefTarget::Callee(cano_expr.find(set.target().span())?),
        ast::Expr::Ident(..) | ast::Expr::MathIdent(..) | ast::Expr::FieldAccess(..) => {
            DerefTarget::VarAccess(cano_expr)
        }
        ast::Expr::Str(..) => {
            let parent = cano_expr.parent()?;
            if parent.kind() == SyntaxKind::ModuleImport {
                DerefTarget::ImportPath(cano_expr)
            } else if parent.kind() == SyntaxKind::ModuleInclude {
                DerefTarget::IncludePath(cano_expr)
            } else {
                DerefTarget::Normal(cano_expr.kind(), cano_expr)
            }
        }
        _ if expr.hash()
            || matches!(cano_expr.kind(), SyntaxKind::MathIdent | SyntaxKind::Error) =>
        {
            DerefTarget::Normal(cano_expr.kind(), cano_expr)
        }
        _ => return None,
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
    log::debug!("def expr: {ancestor:?}");
    let ancestor = deref_lvalue(ancestor)?;
    log::debug!("def lvalue: {ancestor:?}");

    let may_ident = ancestor.cast::<ast::Expr>()?;

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
        _ if may_ident.hash() => return None,
        _ => {
            debug!("unsupported kind {kind:?}", kind = ancestor.kind());
            return None;
        }
    })
}

#[derive(Debug, Clone)]
pub enum ParamTarget<'a> {
    Positional {
        spreads: EcoVec<LinkedNode<'a>>,
        positional: usize,
        is_spread: bool,
    },
    Named(LinkedNode<'a>),
}

#[derive(Debug, Clone)]
pub enum CheckTarget<'a> {
    Param {
        target: ParamTarget<'a>,
        is_set: bool,
    },
    Normal(LinkedNode<'a>),
}

impl<'a> CheckTarget<'a> {
    pub fn node(&self) -> Option<LinkedNode<'a>> {
        Some(match self {
            CheckTarget::Param { target, .. } => match target {
                ParamTarget::Positional { .. } => return None,
                ParamTarget::Named(node) => node.clone(),
            },
            CheckTarget::Normal(node) => node.clone(),
        })
    }
}

pub fn get_check_target(node: LinkedNode) -> Option<CheckTarget<'_>> {
    let mut node = node;
    while node.kind().is_trivia() {
        node = node.prev_sibling()?;
    }

    let deref_target = get_deref_target(node.clone(), node.offset())?;

    match deref_target {
        DerefTarget::Callee(callee) => {
            let parent = callee.parent()?;
            let args = match parent.cast::<ast::Expr>() {
                Some(ast::Expr::FuncCall(call)) => call.args(),
                Some(ast::Expr::Set(set)) => set.args(),
                _ => return None,
            };
            let args_node = node.find(args.span())?;

            let param_target = get_param_target(args_node, node)?;
            Some(CheckTarget::Param {
                target: param_target,
                is_set: parent.kind() == SyntaxKind::Set,
            })
        }
        deref_target => Some(CheckTarget::Normal(deref_target.node().clone())),
    }
}

fn get_param_target<'a>(
    args_node: LinkedNode<'a>,
    node: LinkedNode<'a>,
) -> Option<ParamTarget<'a>> {
    match node.kind() {
        SyntaxKind::Colon => {
            let prev = node.prev_leaf()?;
            let param_ident = prev.cast::<ast::Ident>()?;
            Some(ParamTarget::Named(args_node.find(param_ident.span())?))
        }
        SyntaxKind::Spread | SyntaxKind::Comma | SyntaxKind::LeftParen => {
            let mut spreads = EcoVec::new();
            let mut positional = 0;
            let is_spread = node.kind() == SyntaxKind::Spread;

            let args_before = args_node
                .children()
                .take_while(|arg| arg.range().end <= node.offset());
            for ch in args_before {
                match ch.cast::<ast::Arg>() {
                    Some(ast::Arg::Pos(..)) => {
                        positional += 1;
                    }
                    Some(ast::Arg::Spread(..)) => {
                        spreads.push(ch);
                    }
                    Some(ast::Arg::Named(..)) | None => {}
                }
            }

            Some(ParamTarget::Positional {
                spreads,
                positional,
                is_spread,
            })
        }
        _ => None,
    }
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
