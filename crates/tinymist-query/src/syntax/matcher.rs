use serde::{Deserialize, Serialize};

use crate::prelude::*;

/// Finds the ancestors of a node lazily.
pub fn node_ancestors<'a, 'b>(
    node: &'b LinkedNode<'a>,
) -> impl Iterator<Item = &'b LinkedNode<'a>> {
    std::iter::successors(Some(node), |node| node.parent())
}

/// Finds the expression target.
pub fn deref_expr(node: LinkedNode) -> Option<LinkedNode> {
    node_ancestors(&node).find(|n| n.is::<ast::Expr>()).cloned()
}

/// A descent syntax item.
pub enum DescentItem<'a> {
    /// When the iterator is on a sibling node.
    Sibling(&'a LinkedNode<'a>),
    /// When the iterator is crossing a parent node.
    Parent(&'a LinkedNode<'a>, &'a LinkedNode<'a>),
}

impl<'a> DescentItem<'a> {
    pub fn node(&self) -> &'a LinkedNode<'a> {
        match self {
            DescentItem::Sibling(node) => node,
            DescentItem::Parent(node, _) => node,
        }
    }
}

/// Finds the descent items starting from the given position.
pub fn descent_items<T>(
    node: LinkedNode,
    mut recv: impl FnMut(DescentItem) -> Option<T>,
) -> Option<T> {
    let mut ancestor = Some(node);
    while let Some(node) = &ancestor {
        let mut sibling = Some(node.clone());
        while let Some(node) = &sibling {
            if let Some(v) = recv(DescentItem::Sibling(node)) {
                return Some(v);
            }

            sibling = node.prev_sibling();
        }

        if let Some(parent) = node.parent() {
            if let Some(v) = recv(DescentItem::Parent(parent, node)) {
                return Some(v);
            }

            ancestor = Some(parent.clone());
            continue;
        }

        break;
    }

    None
}

pub enum DescentDecl<'a> {
    Ident(ast::Ident<'a>),
    ImportSource(ast::Expr<'a>),
    ImportAll(ast::ModuleImport<'a>),
}

/// Finds the descent decls starting from the given position.
pub fn descent_decls<T>(
    node: LinkedNode,
    mut recv: impl FnMut(DescentDecl) -> Option<T>,
) -> Option<T> {
    descent_items(node, |node| {
        match (&node, node.node().cast::<ast::Expr>()?) {
            (DescentItem::Sibling(..), ast::Expr::Let(lb)) => {
                for ident in lb.kind().bindings() {
                    if let Some(t) = recv(DescentDecl::Ident(ident)) {
                        return Some(t);
                    }
                }
            }
            (DescentItem::Sibling(..), ast::Expr::Import(mi)) => {
                // import items
                match mi.imports() {
                    Some(ast::Imports::Wildcard) => {
                        if let Some(t) = recv(DescentDecl::ImportAll(mi)) {
                            return Some(t);
                        }
                    }
                    Some(ast::Imports::Items(items)) => {
                        for item in items.iter() {
                            if let Some(t) = recv(DescentDecl::Ident(item.bound_name())) {
                                return Some(t);
                            }
                        }
                    }
                    _ => {}
                }

                // import it self
                if let Some(new_name) = mi.new_name() {
                    if let Some(t) = recv(DescentDecl::Ident(new_name)) {
                        return Some(t);
                    }
                } else if mi.imports().is_none() {
                    if let Some(t) = recv(DescentDecl::ImportSource(mi.source())) {
                        return Some(t);
                    }
                }
            }
            (DescentItem::Parent(node, child), ast::Expr::For(f)) => {
                let body = node.find(f.body().span());
                let in_body = body.is_some_and(|n| n.find(child.span()).is_some());
                if !in_body {
                    return None;
                }

                for ident in f.pattern().bindings() {
                    if let Some(t) = recv(DescentDecl::Ident(ident)) {
                        return Some(t);
                    }
                }
            }
            (DescentItem::Parent(node, child), ast::Expr::Closure(c)) => {
                let body = node.find(c.body().span());
                let in_body = body.is_some_and(|n| n.find(child.span()).is_some());
                if !in_body {
                    return None;
                }

                for param in c.params().children() {
                    match param {
                        ast::Param::Pos(pattern) => {
                            for ident in pattern.bindings() {
                                if let Some(t) = recv(DescentDecl::Ident(ident)) {
                                    return Some(t);
                                }
                            }
                        }
                        ast::Param::Named(n) => {
                            if let Some(t) = recv(DescentDecl::Ident(n.name())) {
                                return Some(t);
                            }
                        }
                        ast::Param::Spread(s) => {
                            if let Some(sink_ident) = s.sink_ident() {
                                if let Some(t) = recv(DescentDecl::Ident(sink_ident)) {
                                    return Some(t);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        };
        None
    })
}

/// Whether the node can be recognized as a mark.
fn is_mark(sk: SyntaxKind) -> bool {
    use SyntaxKind::*;
    #[allow(clippy::match_like_matches_macro)]
    match sk {
        MathAlignPoint | Plus | Minus | Dot | Dots | Arrow | Not | And | Or => true,
        Eq | EqEq | ExclEq | Lt | LtEq | Gt | GtEq | PlusEq | HyphEq | StarEq | SlashEq => true,
        LeftBrace | RightBrace | LeftBracket | RightBracket | LeftParen | RightParen => true,
        Slash | Hat | Comma | Semicolon | Colon | Hash => true,
        _ => false,
    }
}

/// Whether the node can be recognized as an identifier.
pub fn is_ident_like(node: &SyntaxNode) -> bool {
    fn can_be_ident(node: &SyntaxNode) -> bool {
        typst::syntax::is_ident(node.text())
    }

    use SyntaxKind::*;
    let k = node.kind();
    matches!(k, Ident | MathIdent | Underscore)
        || (matches!(k, Error) && can_be_ident(node))
        || k.is_keyword()
}

/// A mode in which a text document is interpreted.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, strum::EnumIter)]
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

/// Determine the interpretation mode at the given position (context-sensitive).
pub(crate) fn interpret_mode_at(mut leaf: Option<&LinkedNode>) -> InterpretMode {
    loop {
        crate::log_debug_ct!("leaf for context: {leaf:?}");
        if let Some(t) = leaf {
            if let Some(mode) = interpret_mode_at_kind(t.kind()) {
                break mode;
            }

            leaf = t.parent();
        } else {
            break InterpretMode::Markup;
        }
    }
}

/// Determine the interpretation mode at the given kind (context-free).
pub(crate) fn interpret_mode_at_kind(k: SyntaxKind) -> Option<InterpretMode> {
    use SyntaxKind::*;
    Some(match k {
        LineComment | BlockComment => InterpretMode::Comment,
        Raw => InterpretMode::Raw,
        Str => InterpretMode::String,
        CodeBlock | Code => InterpretMode::Code,
        ContentBlock | Markup => InterpretMode::Markup,
        Equation | Math => InterpretMode::Math,
        Label | Text | Ident | FieldAccess | Bool | Int | Float | Numeric | Space | Linebreak
        | Parbreak | Escape | Shorthand | SmartQuote | RawLang | RawDelim | RawTrimmed | Hash
        | LeftBrace | RightBrace | LeftBracket | RightBracket | LeftParen | RightParen | Comma
        | Semicolon | Colon | Star | Underscore | Dollar | Plus | Minus | Slash | Hat | Prime
        | Dot | Eq | EqEq | ExclEq | Lt | LtEq | Gt | GtEq | PlusEq | HyphEq | StarEq | SlashEq
        | Dots | Arrow | Root | Not | And | Or | None | Auto | As | Named | Keyed | Error | End => {
            return Option::None
        }
        Strong | Emph | Link | Ref | RefMarker | Heading | HeadingMarker | ListItem
        | ListMarker | EnumItem | EnumMarker | TermItem | TermMarker => InterpretMode::Markup,
        MathIdent | MathAlignPoint | MathDelimited | MathAttach | MathPrimes | MathFrac
        | MathRoot | MathShorthand => InterpretMode::Math,
        Let | Set | Show | Context | If | Else | For | In | While | Break | Continue | Return
        | Import | Include | Args | Spread | Closure | Params | LetBinding | SetRule | ShowRule
        | Contextual | Conditional | WhileLoop | ForLoop | LoopBreak | ModuleImport
        | ImportItems | ImportItemPath | RenamedImportItem | ModuleInclude | LoopContinue
        | FuncReturn | FuncCall | Unary | Binary | Parenthesized | Dict | Array | Destructuring
        | DestructAssignment => InterpretMode::Code,
    })
}

/// Classes of syntax that can be operated on by IDE functionality.
#[derive(Debug, Clone)]
pub enum SyntaxClass<'a> {
    /// A variable access expression.
    ///
    /// It can be either an identifier or a field access.
    VarAccess(LinkedNode<'a>),
    /// A (content) label expression.
    Label {
        node: LinkedNode<'a>,
        is_error: bool,
    },
    /// A (content) reference expression.
    Ref(LinkedNode<'a>),
    /// A callee expression.
    Callee(LinkedNode<'a>),
    /// An import path expression.
    ImportPath(LinkedNode<'a>),
    /// An include path expression.
    IncludePath(LinkedNode<'a>),
    /// Rest kind of **expressions**.
    Normal(SyntaxKind, LinkedNode<'a>),
}

impl<'a> SyntaxClass<'a> {
    pub fn node(&self) -> &LinkedNode<'a> {
        match self {
            SyntaxClass::Label { node, .. }
            | SyntaxClass::Ref(node)
            | SyntaxClass::VarAccess(node)
            | SyntaxClass::Callee(node)
            | SyntaxClass::ImportPath(node)
            | SyntaxClass::IncludePath(node)
            | SyntaxClass::Normal(_, node) => node,
        }
    }

    pub fn label(node: LinkedNode<'a>) -> Self {
        Self::Label {
            node,
            is_error: false,
        }
    }

    pub fn error_as_label(node: LinkedNode<'a>) -> Self {
        Self::Label {
            node,
            is_error: true,
        }
    }
}

/// Classifies the syntax that can be operated on by IDE functionality.
pub fn classify_syntax(node: LinkedNode, cursor: usize) -> Option<SyntaxClass<'_>> {
    if matches!(node.kind(), SyntaxKind::Error) && node.text().starts_with('<') {
        return Some(SyntaxClass::error_as_label(node));
    }

    /// Skips trivia nodes that are on the same line as the cursor.
    fn can_skip_trivia(node: &LinkedNode, cursor: usize) -> bool {
        // A non-trivia node is our target so we stop at it.
        if !node.kind().is_trivia() || !node.parent_kind().is_some_and(possible_in_code_trivia) {
            return false;
        }

        // Get the trivia text before the cursor.
        let pref = node.text().as_bytes();
        let pref = if node.range().contains(&cursor) {
            &pref[..cursor - node.offset()]
        } else {
            pref
        };

        // The deref target should be on the same line as the cursor.
        // Assuming the underlying text is utf-8 encoded, we can check for newlines by
        // looking for b'\n'.
        // todo: if we are in markup mode, we should check if we are at start of node
        !pref.contains(&b'\n')
    }

    // Move to the first non-trivia node before the cursor.
    let mut node = node;
    if can_skip_trivia(&node, cursor) {
        node = node.prev_sibling()?;
    }

    // Move to the first ancestor that is an expression.
    let ancestor = deref_expr(node)?;
    crate::log_debug_ct!("deref expr: {ancestor:?}");

    // Unwrap all parentheses to get the actual expression.
    let cano_expr = classify_lvalue(ancestor)?;
    crate::log_debug_ct!("deref lvalue: {cano_expr:?}");

    // Identify convenient expression kinds.
    let expr = cano_expr.cast::<ast::Expr>()?;
    Some(match expr {
        ast::Expr::Label(..) => SyntaxClass::label(cano_expr),
        ast::Expr::Ref(..) => SyntaxClass::Ref(cano_expr),
        ast::Expr::FuncCall(call) => SyntaxClass::Callee(cano_expr.find(call.callee().span())?),
        ast::Expr::Set(set) => SyntaxClass::Callee(cano_expr.find(set.target().span())?),
        ast::Expr::Ident(..) | ast::Expr::MathIdent(..) | ast::Expr::FieldAccess(..) => {
            SyntaxClass::VarAccess(cano_expr)
        }
        ast::Expr::Str(..) => {
            let parent = cano_expr.parent()?;
            if parent.kind() == SyntaxKind::ModuleImport {
                SyntaxClass::ImportPath(cano_expr)
            } else if parent.kind() == SyntaxKind::ModuleInclude {
                SyntaxClass::IncludePath(cano_expr)
            } else {
                SyntaxClass::Normal(cano_expr.kind(), cano_expr)
            }
        }
        _ if expr.hash()
            || matches!(cano_expr.kind(), SyntaxKind::MathIdent | SyntaxKind::Error) =>
        {
            SyntaxClass::Normal(cano_expr.kind(), cano_expr)
        }
        _ => return None,
    })
}

/// Whether the node might be in code trivia. This is a bit internal so please
/// check the caller to understand it.
fn possible_in_code_trivia(sk: SyntaxKind) -> bool {
    !matches!(
        interpret_mode_at_kind(sk),
        Some(InterpretMode::Markup | InterpretMode::Math | InterpretMode::Comment)
    )
}

/// Finds a more canonical expression target.
/// It is not formal, but the following cases are forbidden:
/// - Parenthesized expression.
/// - Identifier on the right side of a dot operator (field access).
fn classify_lvalue(mut node: LinkedNode) -> Option<LinkedNode> {
    while let Some(paren_expr) = node.cast::<ast::Parenthesized>() {
        node = node.find(paren_expr.expr().span())?;
    }
    if let Some(parent) = node.parent() {
        if let Some(field_access) = parent.cast::<ast::FieldAccess>() {
            if node.span() == field_access.field().span() {
                return Some(parent.clone());
            }
        }
    }
    Some(node)
}

/// Classes of def items that can be operated on by IDE functionality.
#[derive(Debug, Clone)]
pub enum DefClass<'a> {
    /// A let binding item.
    Let(LinkedNode<'a>),
    /// A module import item.
    Import(LinkedNode<'a>),
}

impl DefClass<'_> {
    pub fn node(&self) -> &LinkedNode {
        match self {
            DefClass::Let(node) => node,
            DefClass::Import(node) => node,
        }
    }

    pub fn name_range(&self) -> Option<Range<usize>> {
        self.name().map(|node| node.range())
    }

    pub fn name(&self) -> Option<LinkedNode> {
        match self {
            DefClass::Let(node) => {
                let lb: ast::LetBinding<'_> = node.cast()?;
                let names = match lb.kind() {
                    ast::LetBindingKind::Closure(name) => node.find(name.span())?,
                    ast::LetBindingKind::Normal(ast::Pattern::Normal(name)) => {
                        node.find(name.span())?
                    }
                    _ => return None,
                };

                Some(names)
            }
            DefClass::Import(_node) => {
                // let ident = node.cast::<ast::ImportItem>()?;
                // Some(ident.span().into())
                // todo: implement this
                None
            }
        }
    }
}

// todo: whether we should distinguish between strict and loose def classes
/// Classifies a definition under cursor loosely.
pub fn classify_def_loosely(node: LinkedNode) -> Option<DefClass<'_>> {
    classify_def_(node, false)
}

/// Classifies a definition under cursor strictly.
pub fn classify_def(node: LinkedNode) -> Option<DefClass<'_>> {
    classify_def_(node, true)
}

/// The internal implementation of classifying a definition.
fn classify_def_(node: LinkedNode, strict: bool) -> Option<DefClass<'_>> {
    let mut ancestor = node;
    if ancestor.kind().is_trivia() || is_mark(ancestor.kind()) {
        ancestor = ancestor.prev_sibling()?;
    }

    while !ancestor.is::<ast::Expr>() {
        ancestor = ancestor.parent()?.clone();
    }
    crate::log_debug_ct!("def expr: {ancestor:?}");
    let ancestor = classify_lvalue(ancestor)?;
    crate::log_debug_ct!("def lvalue: {ancestor:?}");

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
        ast::Expr::Let(..) => DefClass::Let(ancestor),
        ast::Expr::Import(..) => DefClass::Import(ancestor),
        // todo: parameter
        ast::Expr::Ident(..)
        | ast::Expr::MathIdent(..)
        | ast::Expr::FieldAccess(..)
        | ast::Expr::Closure(..) => {
            let mut ancestor = ancestor;
            while !ancestor.is::<ast::LetBinding>() {
                ancestor = ancestor.parent()?.clone();
            }

            DefClass::Let(ancestor)
        }
        ast::Expr::Str(..) => {
            let parent = ancestor.parent()?;
            if parent.kind() != SyntaxKind::ModuleImport {
                return None;
            }

            DefClass::Import(parent.clone())
        }
        _ if may_ident.hash() => return None,
        _ => {
            crate::log_debug_ct!("unsupported kind {kind:?}", kind = ancestor.kind());
            return None;
        }
    })
}

/// Classes of arguments that can be operated on by IDE functionality.
#[derive(Debug, Clone)]
pub enum ArgClass<'a> {
    /// A positional argument.
    Positional {
        spreads: EcoVec<LinkedNode<'a>>,
        positional: usize,
        is_spread: bool,
    },
    /// A named argument.
    Named(LinkedNode<'a>),
}

impl ArgClass<'_> {
    pub(crate) fn positional_from_before(before: bool) -> Self {
        ArgClass::Positional {
            spreads: EcoVec::new(),
            positional: if before { 0 } else { 1 },
            is_spread: false,
        }
    }
}

/// Classes of syntax under cursor that are preferred by type checking.
///
/// A cursor class is either an [`SyntaxClass`] or other things under cursor.
/// One thing is not ncessary to refer to some exact node. For example, a cursor
/// moving after some comma in a function call is identified as a
/// [`CursorClass::Arg`].
#[derive(Debug, Clone)]
pub enum CursorClass<'a> {
    /// A cursor on an argument.
    Arg {
        callee: LinkedNode<'a>,
        args: LinkedNode<'a>,
        target: ArgClass<'a>,
        is_set: bool,
    },
    /// A cursor on an element in an array or dictionary literal.
    Element {
        container: LinkedNode<'a>,
        target: ArgClass<'a>,
    },
    /// A cursor on a parenthesized expression.
    Paren {
        container: LinkedNode<'a>,
        is_before: bool,
    },
    /// A cursor on an import path.
    ImportPath(LinkedNode<'a>),
    /// A cursor on an include path.
    IncludePath(LinkedNode<'a>),
    /// A cursor on a label.
    Label {
        node: LinkedNode<'a>,
        is_error: bool,
    },
    /// A cursor on a normal [`SyntaxClass`].
    Normal(LinkedNode<'a>),
}

impl<'a> CursorClass<'a> {
    pub fn node(&self) -> Option<LinkedNode<'a>> {
        Some(match self {
            CursorClass::Arg { target, .. } | CursorClass::Element { target, .. } => match target {
                ArgClass::Positional { .. } => return None,
                ArgClass::Named(node) => node.clone(),
            },
            CursorClass::Paren { container, .. } => container.clone(),
            CursorClass::Label { node, .. }
            | CursorClass::ImportPath(node)
            | CursorClass::IncludePath(node)
            | CursorClass::Normal(node) => node.clone(),
        })
    }
}

/// Kind of argument source.
#[derive(Debug)]
enum ArgSourceKind {
    /// An argument in a function call.
    Call,
    /// An argument (element) in an array literal.
    Array,
    /// An argument (element) in a dictionary literal.
    Dict,
}

/// Classifies a cursor expression by context.
pub fn classify_cursor_by_context<'a>(
    context: LinkedNode<'a>,
    node: LinkedNode<'a>,
) -> Option<CursorClass<'a>> {
    use SyntaxClass::*;
    let context_syntax = classify_syntax(context.clone(), node.offset())?;
    let node_syntax = classify_syntax(node.clone(), node.offset())?;

    match context_syntax {
        Callee(callee)
            if matches!(node_syntax, Normal(..) | Label { .. } | Ref(..))
                && !matches!(node_syntax, Callee(..)) =>
        {
            let parent = callee.parent()?;
            let args = match parent.cast::<ast::Expr>() {
                Some(ast::Expr::FuncCall(call)) => call.args(),
                Some(ast::Expr::Set(set)) => set.args(),
                _ => return None,
            };
            let args = parent.find(args.span())?;

            let is_set = parent.kind() == SyntaxKind::SetRule;
            let arg_target = cursor_on_arg(args.clone(), node, ArgSourceKind::Call)?;
            Some(CursorClass::Arg {
                callee,
                args,
                target: arg_target,
                is_set,
            })
        }
        _ => None,
    }
}

/// Classifies an expression under cursor that are preferred by type checking.
pub fn classify_cursor(node: LinkedNode) -> Option<CursorClass<'_>> {
    let mut node = node;
    if node.kind().is_trivia() && node.parent_kind().is_some_and(possible_in_code_trivia) {
        loop {
            node = node.prev_sibling()?;

            if !node.kind().is_trivia() {
                break;
            }
        }
    }

    let syntax = classify_syntax(node.clone(), node.offset())?;

    let normal_syntax = match syntax {
        SyntaxClass::Callee(callee) => {
            return cursor_on_callee(callee, node);
        }
        SyntaxClass::Label { node, is_error } => {
            return Some(CursorClass::Label { node, is_error });
        }
        SyntaxClass::ImportPath(node) => {
            return Some(CursorClass::ImportPath(node));
        }
        SyntaxClass::IncludePath(node) => {
            return Some(CursorClass::IncludePath(node));
        }
        syntax => syntax.node().clone(),
    };

    let Some(mut node_parent) = node.parent().cloned() else {
        return Some(CursorClass::Normal(node));
    };

    while let SyntaxKind::Named | SyntaxKind::Colon = node_parent.kind() {
        let Some(p) = node_parent.parent() else {
            return Some(CursorClass::Normal(node));
        };
        node_parent = p.clone();
    }

    match node_parent.kind() {
        SyntaxKind::Args => {
            let callee = node_ancestors(&node_parent).find_map(|p| {
                let s = match p.cast::<ast::Expr>()? {
                    ast::Expr::FuncCall(call) => call.callee().span(),
                    ast::Expr::Set(set) => set.target().span(),
                    _ => return None,
                };
                p.find(s)
            })?;

            let param_node = match node.kind() {
                SyntaxKind::Ident
                    if matches!(
                        node.parent_kind().zip(node.next_sibling_kind()),
                        Some((SyntaxKind::Named, SyntaxKind::Colon))
                    ) =>
                {
                    node
                }
                _ if matches!(node.parent_kind(), Some(SyntaxKind::Named)) => {
                    node.parent().cloned()?
                }
                _ => node,
            };

            cursor_on_callee(callee, param_node)
        }
        SyntaxKind::Array | SyntaxKind::Dict => {
            let element_target = cursor_on_arg(
                node_parent.clone(),
                node.clone(),
                match node_parent.kind() {
                    SyntaxKind::Array => ArgSourceKind::Array,
                    SyntaxKind::Dict => ArgSourceKind::Dict,
                    _ => unreachable!(),
                },
            )?;
            Some(CursorClass::Element {
                container: node_parent.clone(),
                target: element_target,
            })
        }
        SyntaxKind::Parenthesized => {
            let is_before = node.offset() <= node_parent.offset() + 1;
            Some(CursorClass::Paren {
                container: node_parent.clone(),
                is_before,
            })
        }
        _ => Some(CursorClass::Normal(normal_syntax)),
    }
}

fn cursor_on_callee<'a>(callee: LinkedNode<'a>, node: LinkedNode<'a>) -> Option<CursorClass<'a>> {
    let parent = callee.parent()?;
    let args = match parent.cast::<ast::Expr>() {
        Some(ast::Expr::FuncCall(call)) => call.args(),
        Some(ast::Expr::Set(set)) => set.args(),
        _ => return None,
    };
    let args = parent.find(args.span())?;

    let is_set = parent.kind() == SyntaxKind::SetRule;
    let target = cursor_on_arg(args.clone(), node, ArgSourceKind::Call)?;
    Some(CursorClass::Arg {
        callee,
        args,
        target,
        is_set,
    })
}

fn cursor_on_arg<'a>(
    args_node: LinkedNode<'a>,
    mut node: LinkedNode<'a>,
    param_kind: ArgSourceKind,
) -> Option<ArgClass<'a>> {
    if node.kind() == SyntaxKind::RightParen {
        node = node.prev_sibling()?;
    }
    match node.kind() {
        SyntaxKind::Named => {
            let param_ident = node.cast::<ast::Named>()?.name();
            Some(ArgClass::Named(args_node.find(param_ident.span())?))
        }
        SyntaxKind::Colon => {
            let prev = node.prev_leaf()?;
            let param_ident = prev.cast::<ast::Ident>()?;
            Some(ArgClass::Named(args_node.find(param_ident.span())?))
        }
        _ => {
            let mut spreads = EcoVec::new();
            let mut positional = 0;
            let is_spread = node.kind() == SyntaxKind::Spread;

            let args_before = args_node
                .children()
                .take_while(|arg| arg.range().end <= node.offset());
            match param_kind {
                ArgSourceKind::Call => {
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
                ArgSourceKind::Array => {
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
                ArgSourceKind::Dict => {
                    for ch in args_before {
                        if let Some(ast::DictItem::Spread(..)) = ch.cast::<ast::DictItem>() {
                            spreads.push(ch);
                        }
                    }
                }
            }

            Some(ArgClass::Positional {
                spreads,
                positional,
                is_spread,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use typst::syntax::{is_newline, Source};
    use typst_shim::syntax::LinkedNodeExt;

    fn map_node(source: &str, mapper: impl Fn(&LinkedNode, usize) -> char) -> String {
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

    fn map_syntax(source: &str) -> String {
        map_node(source, |root, cursor| {
            let node = root.leaf_at_compat(cursor);
            let kind = node.and_then(|node| classify_syntax(node, cursor));
            match kind {
                Some(SyntaxClass::VarAccess(..)) => 'v',
                Some(SyntaxClass::Normal(..)) => 'n',
                Some(SyntaxClass::Label { .. }) => 'l',
                Some(SyntaxClass::Ref(..)) => 'r',
                Some(SyntaxClass::Callee(..)) => 'c',
                Some(SyntaxClass::ImportPath(..)) => 'i',
                Some(SyntaxClass::IncludePath(..)) => 'I',
                None => ' ',
            }
        })
    }

    fn map_cursor(source: &str) -> String {
        map_node(source, |root, cursor| {
            let node = root.leaf_at_compat(cursor);
            let kind = node.and_then(|node| classify_cursor(node));
            match kind {
                Some(CursorClass::Arg { .. }) => 'p',
                Some(CursorClass::Element { .. }) => 'e',
                Some(CursorClass::Paren { .. }) => 'P',
                Some(CursorClass::ImportPath(..)) => 'i',
                Some(CursorClass::IncludePath(..)) => 'I',
                Some(CursorClass::Label { .. }) => 'l',
                Some(CursorClass::Normal(..)) => 'n',
                None => ' ',
            }
        })
    }

    #[test]
    fn test_get_deref_target() {
        assert_snapshot!(map_syntax(r#"#let x = 1  
Text
= Heading #let y = 2;  
== Heading"#).trim(), @r"
        #let x = 1  
         nnnnvvnnn  
        Text
            
        = Heading #let y = 2;  
                   nnnnvvnnn   
        == Heading
        ");
        assert_snapshot!(map_syntax(r#"#let f(x);"#).trim(), @r"
        #let f(x);
         nnnnv v
        ");
        assert_snapshot!(map_syntax(r#"#{
  calc.  
}"#).trim(), @r"
        #{
         n
          calc.  
        nnvvvvvnn
        }
        n
        ");
    }

    #[test]
    fn test_get_check_target() {
        assert_snapshot!(map_cursor(r#"#let x = 1  
Text
= Heading #let y = 2;  
== Heading"#).trim(), @r"
        #let x = 1  
         nnnnnnnnn  
        Text
            
        = Heading #let y = 2;  
                   nnnnnnnnn   
        == Heading
        ");
        assert_snapshot!(map_cursor(r#"#let f(x);"#).trim(), @r"
        #let f(x);
         nnnnn n
        ");
        assert_snapshot!(map_cursor(r#"#f(1, 2)   Test"#).trim(), @r"
        #f(1, 2)   Test
         npppppp
        ");
        assert_snapshot!(map_cursor(r#"#()   Test"#).trim(), @r"
        #()   Test
         ee
        ");
        assert_snapshot!(map_cursor(r#"#(1)   Test"#).trim(), @r"
        #(1)   Test
         PPP
        ");
        assert_snapshot!(map_cursor(r#"#(a: 1)   Test"#).trim(), @r"
        #(a: 1)   Test
         eeeeee
        ");
        assert_snapshot!(map_cursor(r#"#(1, 2)   Test"#).trim(), @r"
        #(1, 2)   Test
         eeeeee
        ");
        assert_snapshot!(map_cursor(r#"#(1, 2)  
  Test"#).trim(), @r"
        #(1, 2)  
         eeeeee  
          Test
        ");
    }
}
