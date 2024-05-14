use ecow::EcoVec;
use serde::Serialize;
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

/// A mode in which a text document is interpreted.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InterpretMode {
    /// The position is in a comment.
    Comment,
    /// The position is in a string.
    String,
    /// The position is in a raw.
    Raw,
    /// The position is in a markup block.
    Markup,
    /// The position is in a code block.
    Code,
    /// The position is in a math equation.
    Math,
}

pub(crate) fn interpret_mode_at(k: SyntaxKind) -> Option<InterpretMode> {
    use SyntaxKind::*;
    Some(match k {
        LineComment | BlockComment => InterpretMode::Comment,
        Raw => InterpretMode::Raw,
        Str => InterpretMode::String,
        CodeBlock | Code => InterpretMode::Code,
        ContentBlock | Markup => InterpretMode::Markup,
        Equation | Math => InterpretMode::Math,
        Ident | FieldAccess | Bool | Int | Float | Numeric | Space | Linebreak | Parbreak
        | Escape | Shorthand | SmartQuote | RawLang | RawDelim | RawTrimmed | Hash | LeftBrace
        | RightBrace | LeftBracket | RightBracket | LeftParen | RightParen | Comma | Semicolon
        | Colon | Star | Underscore | Dollar | Plus | Minus | Slash | Hat | Prime | Dot | Eq
        | EqEq | ExclEq | Lt | LtEq | Gt | GtEq | PlusEq | HyphEq | StarEq | SlashEq | Dots
        | Arrow | Root | Not | And | Or | None | Auto | As | Named | Keyed | Error | Eof => {
            return Option::None
        }
        Text | Strong | Emph | Link | Label | Ref | RefMarker | Heading | HeadingMarker
        | ListItem | ListMarker | EnumItem | EnumMarker | TermItem | TermMarker => {
            InterpretMode::Markup
        }
        MathIdent | MathAlignPoint | MathDelimited | MathAttach | MathPrimes | MathFrac
        | MathRoot => InterpretMode::Math,
        Let | Set | Show | Context | If | Else | For | In | While | Break | Continue | Return
        | Import | Include | Args | Spread | Closure | Params | LetBinding | SetRule | ShowRule
        | Contextual | Conditional | WhileLoop | ForLoop | LoopBreak | ModuleImport
        | ImportItems | RenamedImportItem | ModuleInclude | LoopContinue | FuncReturn
        | FuncCall | Unary | Binary | Parenthesized | Dict | Array | Destructuring
        | DestructAssignment => InterpretMode::Code,
    })
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
    fn can_skip_trivia(node: &LinkedNode, cursor: usize) -> bool {
        // A non-trivia node is our target so we stop at it.
        if !node.kind().is_trivia() || !node.parent_kind().is_some_and(possible_in_code_trivia) {
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
    if can_skip_trivia(&node, cursor) {
        node = node.prev_sibling()?;
    }

    // Move to the first ancestor that is an expression.
    let mut ancestor = node;
    while !ancestor.is::<ast::Expr>() {
        ancestor = ancestor.parent()?.clone();
    }
    log::debug!("deref expr: {ancestor:?}");

    // Unwrap all parentheses to get the actual expression.
    let cano_expr = deref_lvalue(ancestor)?;
    log::debug!("deref lvalue: {cano_expr:?}");

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

// todo: whether we should distinguish between strict and non-strict def targets
pub fn get_non_strict_def_target(node: LinkedNode) -> Option<DefTarget<'_>> {
    get_def_target_(node, false)
}

pub fn get_def_target(node: LinkedNode) -> Option<DefTarget<'_>> {
    get_def_target_(node, true)
}

fn get_def_target_(node: LinkedNode, strict: bool) -> Option<DefTarget<'_>> {
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
    if strict && !may_ident.hash() && !matches!(may_ident, ast::Expr::MathIdent(_)) {
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
        _ if may_ident.hash() => return None,
        _ => {
            log::debug!("unsupported kind {kind:?}", kind = ancestor.kind());
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
impl<'a> ParamTarget<'a> {
    pub(crate) fn positional_from_before(before: bool) -> Self {
        ParamTarget::Positional {
            spreads: EcoVec::new(),
            positional: if before { 0 } else { 1 },
            is_spread: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CheckTarget<'a> {
    Param {
        callee: LinkedNode<'a>,
        args: LinkedNode<'a>,
        target: ParamTarget<'a>,
        is_set: bool,
    },
    Element {
        container: LinkedNode<'a>,
        target: ParamTarget<'a>,
    },
    Paren {
        container: LinkedNode<'a>,
        is_before: bool,
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
            CheckTarget::Element { target, .. } => match target {
                ParamTarget::Positional { .. } => return None,
                ParamTarget::Named(node) => node.clone(),
            },
            CheckTarget::Paren { container, .. } => container.clone(),
            CheckTarget::Normal(node) => node.clone(),
        })
    }
}

#[derive(Debug)]
enum ParamKind {
    Call,
    Array,
    Dict,
}

pub fn get_check_target_by_context<'a>(
    context: LinkedNode<'a>,
    node: LinkedNode<'a>,
) -> Option<CheckTarget<'a>> {
    let context_deref_target = get_deref_target(context.clone(), node.offset())?;
    let node_deref_target = get_deref_target(node.clone(), node.offset())?;

    match context_deref_target {
        DerefTarget::Callee(callee)
            if matches!(node_deref_target, DerefTarget::Normal(..))
                && !matches!(node_deref_target, DerefTarget::Callee(..)) =>
        {
            let parent = callee.parent()?;
            let args = match parent.cast::<ast::Expr>() {
                Some(ast::Expr::FuncCall(call)) => call.args(),
                Some(ast::Expr::Set(set)) => set.args(),
                _ => return None,
            };
            let args = parent.find(args.span())?;

            let is_set = parent.kind() == SyntaxKind::SetRule;
            let target = get_param_target(args.clone(), node, ParamKind::Call)?;
            Some(CheckTarget::Param {
                callee,
                args,
                target,
                is_set,
            })
        }
        _ => None,
    }
}

fn possible_in_code_trivia(sk: SyntaxKind) -> bool {
    !matches!(
        interpret_mode_at(sk),
        Some(InterpretMode::Markup | InterpretMode::Math | InterpretMode::Comment)
    )
}

pub fn get_check_target(node: LinkedNode) -> Option<CheckTarget<'_>> {
    let mut node = node;
    if node.kind().is_trivia() && node.parent_kind().is_some_and(possible_in_code_trivia) {
        loop {
            node = node.prev_sibling()?;

            if !node.kind().is_trivia() {
                break;
            }
        }
    }

    let deref_target = get_deref_target(node.clone(), node.offset())?;

    let deref_node = match deref_target {
        DerefTarget::Callee(callee) => {
            let parent = callee.parent()?;
            let args = match parent.cast::<ast::Expr>() {
                Some(ast::Expr::FuncCall(call)) => call.args(),
                Some(ast::Expr::Set(set)) => set.args(),
                _ => return None,
            };
            let args = parent.find(args.span())?;

            let is_set = parent.kind() == SyntaxKind::SetRule;
            let target = get_param_target(args.clone(), node, ParamKind::Call)?;
            return Some(CheckTarget::Param {
                callee,
                args,
                target,
                is_set,
            });
        }
        DerefTarget::ImportPath(node) | DerefTarget::IncludePath(node) => {
            return Some(CheckTarget::Normal(node));
        }
        deref_target => deref_target.node().clone(),
    };

    let Some(mut node_parent) = node.parent() else {
        return Some(CheckTarget::Normal(node));
    };

    while let SyntaxKind::Named | SyntaxKind::Colon = node_parent.kind() {
        let Some(p) = node_parent.parent() else {
            return Some(CheckTarget::Normal(node));
        };
        node_parent = p;
    }

    match node_parent.kind() {
        SyntaxKind::Array | SyntaxKind::Dict => {
            let target = get_param_target(
                node_parent.clone(),
                node.clone(),
                match node_parent.kind() {
                    SyntaxKind::Array => ParamKind::Array,
                    SyntaxKind::Dict => ParamKind::Dict,
                    _ => unreachable!(),
                },
            )?;
            Some(CheckTarget::Element {
                container: node_parent.clone(),
                target,
            })
        }
        SyntaxKind::Parenthesized => {
            let is_before = node.offset() <= node_parent.offset() + 1;
            Some(CheckTarget::Paren {
                container: node_parent.clone(),
                is_before,
            })
        }
        _ => Some(CheckTarget::Normal(deref_node)),
    }
}

fn get_param_target<'a>(
    args_node: LinkedNode<'a>,
    mut node: LinkedNode<'a>,
    param_kind: ParamKind,
) -> Option<ParamTarget<'a>> {
    if node.kind() == SyntaxKind::RightParen {
        node = node.prev_sibling()?;
    }
    match node.kind() {
        SyntaxKind::Named => {
            let param_ident = node.cast::<ast::Named>()?.name();
            Some(ParamTarget::Named(args_node.find(param_ident.span())?))
        }
        SyntaxKind::Colon => {
            let prev = node.prev_leaf()?;
            let param_ident = prev.cast::<ast::Ident>()?;
            Some(ParamTarget::Named(args_node.find(param_ident.span())?))
        }
        _ => {
            let mut spreads = EcoVec::new();
            let mut positional = 0;
            let is_spread = node.kind() == SyntaxKind::Spread;

            let args_before = args_node
                .children()
                .take_while(|arg| arg.range().end <= node.offset());
            match param_kind {
                ParamKind::Call => {
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
                }
                ParamKind::Array => {
                    for ch in args_before {
                        match ch.cast::<ast::ArrayItem>() {
                            Some(ast::ArrayItem::Pos(..)) => {
                                positional += 1;
                            }
                            Some(ast::ArrayItem::Spread(..)) => {
                                spreads.push(ch);
                            }
                            _ => {}
                        }
                    }
                }
                ParamKind::Dict => {
                    for ch in args_before {
                        if let Some(ast::DictItem::Spread(..)) = ch.cast::<ast::DictItem>() {
                            spreads.push(ch);
                        }
                    }
                }
            }

            Some(ParamTarget::Positional {
                spreads,
                positional,
                is_spread,
            })
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use typst::syntax::{is_newline, Source};

    fn map_base(source: &str, mapper: impl Fn(&LinkedNode, usize) -> char) -> String {
        let source = Source::detached(source.to_owned());
        let root = LinkedNode::new(source.root());
        let mut output_mapping = String::new();

        let mut cursor = 0;
        for ch in source.text().chars() {
            cursor += ch.len_utf8();
            if is_newline(ch) {
                output_mapping.push(ch);
                continue;
            }

            output_mapping.push(mapper(&root, cursor));
        }

        source
            .text()
            .lines()
            .zip(output_mapping.lines())
            .flat_map(|(a, b)| [a, "\n", b, "\n"])
            .collect::<String>()
    }

    fn map_deref(source: &str) -> String {
        map_base(source, |root, cursor| {
            let node = root.leaf_at(cursor);
            let kind = node.and_then(|node| get_deref_target(node, cursor));
            match kind {
                Some(DerefTarget::VarAccess(..)) => 'v',
                Some(DerefTarget::Normal(..)) => 'n',
                Some(DerefTarget::Label(..)) => 'l',
                Some(DerefTarget::Ref(..)) => 'r',
                Some(DerefTarget::Callee(..)) => 'c',
                Some(DerefTarget::ImportPath(..)) => 'i',
                Some(DerefTarget::IncludePath(..)) => 'I',
                None => ' ',
            }
        })
    }

    fn map_check(source: &str) -> String {
        map_base(source, |root, cursor| {
            let node = root.leaf_at(cursor);
            let kind = node.and_then(|node| get_check_target(node));
            match kind {
                Some(CheckTarget::Param { .. }) => 'p',
                Some(CheckTarget::Element { .. }) => 'e',
                Some(CheckTarget::Paren { .. }) => 'P',
                Some(CheckTarget::Normal(..)) => 'n',
                None => ' ',
            }
        })
    }

    #[test]
    fn test_get_deref_target() {
        assert_snapshot!(map_deref(r#"#let x = 1  
Text
= Heading #let y = 2;  
== Heading"#).trim(), @r###"
        #let x = 1  
         nnnnvvnnn  
        Text
            
        = Heading #let y = 2;  
                   nnnnvvnnn   
        == Heading
        "###);
        assert_snapshot!(map_deref(r#"#let f(x);"#).trim(), @r###"
        #let f(x);
         nnnnv v
        "###);
    }

    #[test]
    fn test_get_check_target() {
        assert_snapshot!(map_check(r#"#let x = 1  
Text
= Heading #let y = 2;  
== Heading"#).trim(), @r###"
        #let x = 1  
         nnnnnnnnn  
        Text
            
        = Heading #let y = 2;  
                   nnnnnnnnn   
        == Heading
        "###);
        assert_snapshot!(map_check(r#"#let f(x);"#).trim(), @r###"
        #let f(x);
         nnnnn n
        "###);
        assert_snapshot!(map_check(r#"#f(1, 2)   Test"#).trim(), @r###"
        #f(1, 2)   Test
         npnppnp
        "###);
        assert_snapshot!(map_check(r#"#()   Test"#).trim(), @r###"
        #()   Test
         ee
        "###);
        assert_snapshot!(map_check(r#"#(1)   Test"#).trim(), @r###"
        #(1)   Test
         PPP
        "###);
        assert_snapshot!(map_check(r#"#(a: 1)   Test"#).trim(), @r###"
        #(a: 1)   Test
         eeeeee
        "###);
        assert_snapshot!(map_check(r#"#(1, 2)   Test"#).trim(), @r###"
        #(1, 2)   Test
         eeeeee
        "###);
        assert_snapshot!(map_check(r#"#(1, 2)  
  Test"#).trim(), @r###"
        #(1, 2)  
         eeeeee  
          Test
        "###);
    }
}
