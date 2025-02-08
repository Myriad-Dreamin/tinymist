//! Convenient utilities to match syntax structures of code.
//! - Iterators/Finders to traverse nodes.
//! - Predicates to check nodes' properties.
//! - Classifiers to check nodes' syntax.
//!
//! ## Classifiers of syntax structures
//!
//! A node can have a quadruple to describe its syntax:
//!
//! ```text
//! (InterpretMode, SurroundingSyntax/SyntaxContext, DefClass/SyntaxClass, SyntaxNode)
//! ```
//!
//! Among them, [`InterpretMode`], [`SurroundingSyntax`], and [`SyntaxContext`]
//! describes outer syntax. [`DefClass`], [`SyntaxClass`] and
//! [`typst::syntax::SyntaxNode`] describes inner syntax.
//!
//! - [`typst::syntax::SyntaxNode`]: Its contextual version is
//!   [`typst::syntax::LinkedNode`], containing AST information, like inner text
//!   and [`SyntaxKind`], on the position.
//! - [`SyntaxClass`]: Provided by [`classify_syntax`], it describes the
//!   context-free syntax of the node that are more suitable for IDE operations.
//!   For example, it identifies users' half-typed syntax like half-completed
//!   labels and dot accesses.
//! - [`DefClass`]: Provided by [`classify_def`], it describes the definition
//!   class of the node at the position. The difference between `SyntaxClass`
//!   and `DefClass` is that the latter matcher will skip the nodes that do not
//!   define a definition.
//! - [`SyntaxContext`]: Provided by [`classify_context`], it describes the
//!   outer syntax of the node that are more suitable for IDE operations. For
//!   example, it identifies the context of a cursor on the comma in a function
//!   call.
//! - [`SurroundingSyntax`]: Provided by [`surrounding_syntax`], it describes
//!   the surrounding syntax of the node that are more suitable for IDE
//!   operations. The difference between `SyntaxContext` and `SurroundingSyntax`
//!   is that the former is more specific and the latter is more general can be
//!   used for filtering customized snippets.
//! - [`InterpretMode`]: Provided by [`interpret_mode_at`], it describes the how
//!   an interpreter should interpret the code at the position.
//!
//! Some examples of the quadruple (the cursor is marked by `|`):
//!
//! ```text
//! #(x|);
//!    ^ SyntaxContext::Paren, SyntaxClass::Normal(SyntaxKind::Ident)
//! #(x,|);
//!     ^ SyntaxContext::Element, SyntaxClass::Normal(SyntaxKind::Array)
//! #f(x,|);
//!      ^ SyntaxContext::Arg, SyntaxClass::Normal(SyntaxKind::FuncCall)
//! ```
//!
//! ```text
//! #show raw|: |it => it|
//!          ^ SurroundingSyntax::Selector
//!             ^ SurroundingSyntax::ShowTransform
//!                      ^ SurroundingSyntax::Regular
//! ```

use crate::debug_loc::SourceSpanOffset;
use serde::{Deserialize, Serialize};
use typst::syntax::Span;

use crate::prelude::*;

/// Returns the ancestor iterator of the given node.
pub fn node_ancestors<'a, 'b>(
    node: &'b LinkedNode<'a>,
) -> impl Iterator<Item = &'b LinkedNode<'a>> {
    std::iter::successors(Some(node), |node| node.parent())
}

/// Finds the first ancestor node that is an expression.
pub fn first_ancestor_expr(node: LinkedNode) -> Option<LinkedNode> {
    node_ancestors(&node).find(|n| n.is::<ast::Expr>()).cloned()
}

/// A node that is an ancestor of the given node or the previous sibling
/// of some ancestor.
pub enum PreviousItem<'a> {
    /// When the iterator is crossing an ancesstor node.
    Parent(&'a LinkedNode<'a>, &'a LinkedNode<'a>),
    /// When the iterator is on a sibling node of some ancestor.
    Sibling(&'a LinkedNode<'a>),
}

impl<'a> PreviousItem<'a> {
    /// Gets the underlying [`LinkedNode`] of the item.
    pub fn node(&self) -> &'a LinkedNode<'a> {
        match self {
            PreviousItem::Sibling(node) => node,
            PreviousItem::Parent(node, _) => node,
        }
    }
}

/// Finds the previous items (in the scope) starting from the given position
/// inclusively. See [`PreviousItem`] for the possible items.
pub fn previous_items<T>(
    node: LinkedNode,
    mut recv: impl FnMut(PreviousItem) -> Option<T>,
) -> Option<T> {
    let mut ancestor = Some(node);
    while let Some(node) = &ancestor {
        let mut sibling = Some(node.clone());
        while let Some(node) = &sibling {
            if let Some(v) = recv(PreviousItem::Sibling(node)) {
                return Some(v);
            }

            sibling = node.prev_sibling();
        }

        if let Some(parent) = node.parent() {
            if let Some(v) = recv(PreviousItem::Parent(parent, node)) {
                return Some(v);
            }

            ancestor = Some(parent.clone());
            continue;
        }

        break;
    }

    None
}

/// A declaration that is an ancestor of the given node or the previous sibling
/// of some ancestor.
pub enum PreviousDecl<'a> {
    /// An declaration having an identifier.
    ///
    /// ## Example
    ///
    /// The `x` in the following code:
    ///
    /// ```typst
    /// #let x = 1;
    /// ```
    Ident(ast::Ident<'a>),
    /// An declaration yielding from an import source.
    ///
    /// ## Example
    ///
    /// The `x` in the following code:
    ///
    /// ```typst
    /// #import "path.typ": x;
    /// ```
    ImportSource(ast::Expr<'a>),
    /// A wildcard import that possibly containing visible declarations.
    ///
    /// ## Example
    ///
    /// The following import is matched:
    ///
    /// ```typst
    /// #import "path.typ": *;
    /// ```
    ImportAll(ast::ModuleImport<'a>),
}

/// Finds the previous declarations starting from the given position. It checks
/// [`PreviousItem`] and returns the found declarations.
pub fn previous_decls<T>(
    node: LinkedNode,
    mut recv: impl FnMut(PreviousDecl) -> Option<T>,
) -> Option<T> {
    previous_items(node, |item| {
        match (&item, item.node().cast::<ast::Expr>()?) {
            (PreviousItem::Sibling(..), ast::Expr::Let(lb)) => {
                for ident in lb.kind().bindings() {
                    if let Some(t) = recv(PreviousDecl::Ident(ident)) {
                        return Some(t);
                    }
                }
            }
            (PreviousItem::Sibling(..), ast::Expr::Import(import)) => {
                // import items
                match import.imports() {
                    Some(ast::Imports::Wildcard) => {
                        if let Some(t) = recv(PreviousDecl::ImportAll(import)) {
                            return Some(t);
                        }
                    }
                    Some(ast::Imports::Items(items)) => {
                        for item in items.iter() {
                            if let Some(t) = recv(PreviousDecl::Ident(item.bound_name())) {
                                return Some(t);
                            }
                        }
                    }
                    _ => {}
                }

                // import it self
                if let Some(new_name) = import.new_name() {
                    if let Some(t) = recv(PreviousDecl::Ident(new_name)) {
                        return Some(t);
                    }
                } else if import.imports().is_none() {
                    if let Some(t) = recv(PreviousDecl::ImportSource(import.source())) {
                        return Some(t);
                    }
                }
            }
            (PreviousItem::Parent(parent, child), ast::Expr::For(for_expr)) => {
                let body = parent.find(for_expr.body().span());
                let in_body = body.is_some_and(|n| n.find(child.span()).is_some());
                if !in_body {
                    return None;
                }

                for ident in for_expr.pattern().bindings() {
                    if let Some(t) = recv(PreviousDecl::Ident(ident)) {
                        return Some(t);
                    }
                }
            }
            (PreviousItem::Parent(parent, child), ast::Expr::Closure(closure)) => {
                let body = parent.find(closure.body().span());
                let in_body = body.is_some_and(|n| n.find(child.span()).is_some());
                if !in_body {
                    return None;
                }

                for param in closure.params().children() {
                    match param {
                        ast::Param::Pos(pos) => {
                            for ident in pos.bindings() {
                                if let Some(t) = recv(PreviousDecl::Ident(ident)) {
                                    return Some(t);
                                }
                            }
                        }
                        ast::Param::Named(named) => {
                            if let Some(t) = recv(PreviousDecl::Ident(named.name())) {
                                return Some(t);
                            }
                        }
                        ast::Param::Spread(spread) => {
                            if let Some(sink_ident) = spread.sink_ident() {
                                if let Some(t) = recv(PreviousDecl::Ident(sink_ident)) {
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
pub fn is_mark(sk: SyntaxKind) -> bool {
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
    let kind = node.kind();
    matches!(kind, Ident | MathIdent | Underscore)
        || (matches!(kind, Error) && can_be_ident(node))
        || kind.is_keyword()
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
pub fn interpret_mode_at(mut leaf: Option<&LinkedNode>) -> InterpretMode {
    loop {
        crate::log_debug_ct!("leaf for mode: {leaf:?}");
        if let Some(t) = leaf {
            if let Some(mode) = interpret_mode_at_kind(t.kind()) {
                break mode;
            }

            if !t.kind().is_trivia() && {
                // Previous leaf is hash
                t.prev_leaf().is_some_and(|n| n.kind() == SyntaxKind::Hash)
            } {
                return InterpretMode::Code;
            }

            leaf = t.parent();
        } else {
            break InterpretMode::Markup;
        }
    }
}

/// Determine the interpretation mode at the given kind (context-free).
pub(crate) fn interpret_mode_at_kind(kind: SyntaxKind) -> Option<InterpretMode> {
    use SyntaxKind::*;
    Some(match kind {
        LineComment | BlockComment | Shebang => InterpretMode::Comment,
        Raw => InterpretMode::Raw,
        Str => InterpretMode::String,
        CodeBlock | Code => InterpretMode::Code,
        ContentBlock | Markup => InterpretMode::Markup,
        Equation | Math => InterpretMode::Math,
        Hash => InterpretMode::Code,
        Label | Text | Ident | Args | FuncCall | FieldAccess | Bool | Int | Float | Numeric
        | Space | Linebreak | Parbreak | Escape | Shorthand | SmartQuote | RawLang | RawDelim
        | RawTrimmed | LeftBrace | RightBrace | LeftBracket | RightBracket | LeftParen
        | RightParen | Comma | Semicolon | Colon | Star | Underscore | Dollar | Plus | Minus
        | Slash | Hat | Prime | Dot | Eq | EqEq | ExclEq | Lt | LtEq | Gt | GtEq | PlusEq
        | HyphEq | StarEq | SlashEq | Dots | Arrow | Root | Not | And | Or | None | Auto | As
        | Named | Keyed | Spread | Error | End => return Option::None,
        Strong | Emph | Link | Ref | RefMarker | Heading | HeadingMarker | ListItem
        | ListMarker | EnumItem | EnumMarker | TermItem | TermMarker => InterpretMode::Markup,
        MathIdent | MathAlignPoint | MathDelimited | MathAttach | MathPrimes | MathFrac
        | MathRoot | MathShorthand | MathText => InterpretMode::Math,
        Let | Set | Show | Context | If | Else | For | In | While | Break | Continue | Return
        | Import | Include | Closure | Params | LetBinding | SetRule | ShowRule | Contextual
        | Conditional | WhileLoop | ForLoop | LoopBreak | ModuleImport | ImportItems
        | ImportItemPath | RenamedImportItem | ModuleInclude | LoopContinue | FuncReturn
        | Unary | Binary | Parenthesized | Dict | Array | Destructuring | DestructAssignment => {
            InterpretMode::Code
        }
    })
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
    /// Gets the node of the def class.
    pub fn node(&self) -> &LinkedNode {
        match self {
            DefClass::Let(node) => node,
            DefClass::Import(node) => node,
        }
    }

    /// Gets the name node of the def class.
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

    /// Gets the name's range in code of the def class.
    pub fn name_range(&self) -> Option<Range<usize>> {
        self.name().map(|node| node.range())
    }
}

// todo: whether we should distinguish between strict and loose def classes
/// Classifies a definition loosely.
pub fn classify_def_loosely(node: LinkedNode) -> Option<DefClass<'_>> {
    classify_def_(node, false)
}

/// Classifies a definition strictly.
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
    crate::log_debug_ct!("ancestor: {ancestor:?}");
    let adjusted = adjust_expr(ancestor)?;
    crate::log_debug_ct!("adjust_expr: {adjusted:?}");

    let may_ident = adjusted.cast::<ast::Expr>()?;
    if strict && !may_ident.hash() && !matches!(may_ident, ast::Expr::MathIdent(_)) {
        return None;
    }

    let expr = may_ident;
    Some(match expr {
        // todo: label, reference
        // todo: include
        ast::Expr::FuncCall(..) => return None,
        ast::Expr::Set(..) => return None,
        ast::Expr::Let(..) => DefClass::Let(adjusted),
        ast::Expr::Import(..) => DefClass::Import(adjusted),
        // todo: parameter
        ast::Expr::Ident(..)
        | ast::Expr::MathIdent(..)
        | ast::Expr::FieldAccess(..)
        | ast::Expr::Closure(..) => {
            let mut ancestor = adjusted;
            while !ancestor.is::<ast::LetBinding>() {
                ancestor = ancestor.parent()?.clone();
            }

            DefClass::Let(ancestor)
        }
        ast::Expr::Str(..) => {
            let parent = adjusted.parent()?;
            if parent.kind() != SyntaxKind::ModuleImport {
                return None;
            }

            DefClass::Import(parent.clone())
        }
        _ if expr.hash() => return None,
        _ => {
            crate::log_debug_ct!("unsupported kind {:?}", adjusted.kind());
            return None;
        }
    })
}

/// Adjusts an expression node to a more suitable one for classification.
/// It is not formal, but the following cases are forbidden:
/// - Parenthesized expression.
/// - Identifier on the right side of a dot operator (field access).
fn adjust_expr(mut node: LinkedNode) -> Option<LinkedNode> {
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

/// Classes of field syntax that can be operated on by IDE functionality.
#[derive(Debug, Clone)]
pub enum FieldClass<'a> {
    /// A field node.
    ///
    /// ## Example
    ///
    /// The `x` in the following code:
    ///
    /// ```typst
    /// #a.x
    /// ```
    Field(LinkedNode<'a>),

    /// A dot suffix missing a field.
    ///
    /// ## Example
    ///
    /// The `.` in the following code:
    ///
    /// ```typst
    /// #a.
    /// ```
    DotSuffix(SourceSpanOffset),
}

impl FieldClass<'_> {
    /// Gets the node of the field class.
    pub fn offset(&self, source: &Source) -> Option<usize> {
        Some(match self {
            Self::Field(node) => node.offset(),
            Self::DotSuffix(span_offset) => {
                source.find(span_offset.span)?.offset() + span_offset.offset
            }
        })
    }
}

/// Classes of variable (access) syntax that can be operated on by IDE
/// functionality.
#[derive(Debug, Clone)]
pub enum VarClass<'a> {
    /// An identifier expression.
    Ident(LinkedNode<'a>),
    /// A field access expression.
    FieldAccess(LinkedNode<'a>),
    /// A dot access expression, for example, `#a.|`, `$a.|$`, or `x.|.y`.
    /// Note the cursor of the last example is on the middle of the spread
    /// operator.
    DotAccess(LinkedNode<'a>),
}

impl<'a> VarClass<'a> {
    /// Gets the node of the var (access) class.
    pub fn node(&self) -> &LinkedNode<'a> {
        match self {
            Self::Ident(node) | Self::FieldAccess(node) | Self::DotAccess(node) => node,
        }
    }

    /// Gets the accessed node of the var (access) class.
    pub fn accessed_node(&self) -> Option<LinkedNode<'a>> {
        Some(match self {
            Self::Ident(node) => node.clone(),
            Self::FieldAccess(node) => {
                let field_access = node.cast::<ast::FieldAccess>()?;
                node.find(field_access.target().span())?
            }
            Self::DotAccess(node) => node.clone(),
        })
    }

    /// Gets the accessing field of the var (access) class.
    pub fn accessing_field(&self) -> Option<FieldClass<'a>> {
        match self {
            Self::FieldAccess(node) => {
                let dot = node
                    .children()
                    .find(|n| matches!(n.kind(), SyntaxKind::Dot))?;
                let mut iter_after_dot =
                    node.children().skip_while(|n| n.kind() != SyntaxKind::Dot);
                let ident = iter_after_dot.find(|n| {
                    matches!(
                        n.kind(),
                        SyntaxKind::Ident | SyntaxKind::MathIdent | SyntaxKind::Error
                    )
                });

                let ident_case = ident.map(|ident| {
                    if ident.text().is_empty() {
                        FieldClass::DotSuffix(SourceSpanOffset {
                            span: ident.span(),
                            offset: 0,
                        })
                    } else {
                        FieldClass::Field(ident)
                    }
                });

                ident_case.or_else(|| {
                    Some(FieldClass::DotSuffix(SourceSpanOffset {
                        span: dot.span(),
                        offset: 1,
                    }))
                })
            }
            Self::DotAccess(node) => Some(FieldClass::DotSuffix(SourceSpanOffset {
                span: node.span(),
                offset: node.range().len() + 1,
            })),
            Self::Ident(_) => None,
        }
    }
}

/// Classes of syntax that can be operated on by IDE functionality.
#[derive(Debug, Clone)]
pub enum SyntaxClass<'a> {
    /// A variable access expression.
    ///
    /// It can be either an identifier or a field access.
    VarAccess(VarClass<'a>),
    /// A (content) label expression.
    Label {
        /// The node of the label.
        node: LinkedNode<'a>,
        /// Whether the label is converted from an error node.
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
    /// Creates a label syntax class.
    pub fn label(node: LinkedNode<'a>) -> Self {
        Self::Label {
            node,
            is_error: false,
        }
    }

    /// Creates an error label syntax class.
    pub fn error_as_label(node: LinkedNode<'a>) -> Self {
        Self::Label {
            node,
            is_error: true,
        }
    }

    /// Gets the node of the syntax class.
    pub fn node(&self) -> &LinkedNode<'a> {
        match self {
            SyntaxClass::VarAccess(cls) => cls.node(),
            SyntaxClass::Label { node, .. }
            | SyntaxClass::Ref(node)
            | SyntaxClass::Callee(node)
            | SyntaxClass::ImportPath(node)
            | SyntaxClass::IncludePath(node)
            | SyntaxClass::Normal(_, node) => node,
        }
    }

    /// Gets the content offset at which the completion should be triggered.
    pub fn complete_offset(&self) -> Option<usize> {
        match self {
            // `<label`
            //   ^ node.offset() + 1
            SyntaxClass::Label { node, .. } => Some(node.offset() + 1),
            _ => None,
        }
    }
}

/// Classifies node's syntax (inner syntax) that can be operated on by IDE
/// functionality.
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

        // Gets the trivia text before the cursor.
        let previous_text = node.text().as_bytes();
        let previous_text = if node.range().contains(&cursor) {
            &previous_text[..cursor - node.offset()]
        } else {
            previous_text
        };

        // The deref target should be on the same line as the cursor.
        // Assuming the underlying text is utf-8 encoded, we can check for newlines by
        // looking for b'\n'.
        // todo: if we are in markup mode, we should check if we are at start of node
        !previous_text.contains(&b'\n')
    }

    // Moves to the first non-trivia node before the cursor.
    let mut node = node;
    if can_skip_trivia(&node, cursor) {
        node = node.prev_sibling()?;
    }

    /// Matches complete or incomplete dot accesses in code, math, and markup
    /// mode.
    ///
    /// When in markup mode, the dot access is valid if the dot is after a hash
    /// expression.
    fn classify_dot_access<'a>(node: &LinkedNode<'a>) -> Option<SyntaxClass<'a>> {
        let dot_target = node.prev_leaf().and_then(first_ancestor_expr)?;
        let mode = interpret_mode_at(Some(node));

        if matches!(mode, InterpretMode::Math | InterpretMode::Code) || {
            matches!(mode, InterpretMode::Markup)
                && matches!(
                    dot_target.prev_leaf().as_deref().map(SyntaxNode::kind),
                    Some(SyntaxKind::Hash)
                )
        } {
            return Some(SyntaxClass::VarAccess(VarClass::DotAccess(dot_target)));
        }

        None
    }

    if node.offset() + 1 == cursor && {
        // Check if the cursor is exactly after single dot.
        matches!(node.kind(), SyntaxKind::Dot)
            || (matches!(
                node.kind(),
                SyntaxKind::Text | SyntaxKind::MathText | SyntaxKind::Error
            ) && node.text().starts_with("."))
    } {
        if let Some(dot_access) = classify_dot_access(&node) {
            return Some(dot_access);
        }
    }

    if node.offset() + 1 == cursor
        && matches!(node.kind(), SyntaxKind::Dots)
        && matches!(node.parent_kind(), Some(SyntaxKind::Spread))
    {
        if let Some(dot_access) = classify_dot_access(&node) {
            return Some(dot_access);
        }
    }

    if matches!(node.kind(), SyntaxKind::Text) {
        let mode = interpret_mode_at(Some(&node));
        if matches!(mode, InterpretMode::Math) && is_ident_like(&node) {
            return Some(SyntaxClass::VarAccess(VarClass::Ident(node)));
        }
    }

    // Move to the first ancestor that is an expression.
    let ancestor = first_ancestor_expr(node)?;
    crate::log_debug_ct!("first_ancestor_expr: {ancestor:?}");

    // Unwrap all parentheses to get the actual expression.
    let adjusted = adjust_expr(ancestor)?;
    crate::log_debug_ct!("adjust_expr: {adjusted:?}");

    // Identify convenient expression kinds.
    let expr = adjusted.cast::<ast::Expr>()?;
    Some(match expr {
        ast::Expr::Label(..) => SyntaxClass::label(adjusted),
        ast::Expr::Ref(..) => SyntaxClass::Ref(adjusted),
        ast::Expr::FuncCall(call) => SyntaxClass::Callee(adjusted.find(call.callee().span())?),
        ast::Expr::Set(set) => SyntaxClass::Callee(adjusted.find(set.target().span())?),
        ast::Expr::Ident(..) | ast::Expr::MathIdent(..) => {
            SyntaxClass::VarAccess(VarClass::Ident(adjusted))
        }
        ast::Expr::FieldAccess(..) => SyntaxClass::VarAccess(VarClass::FieldAccess(adjusted)),
        ast::Expr::Str(..) => {
            let parent = adjusted.parent()?;
            if parent.kind() == SyntaxKind::ModuleImport {
                SyntaxClass::ImportPath(adjusted)
            } else if parent.kind() == SyntaxKind::ModuleInclude {
                SyntaxClass::IncludePath(adjusted)
            } else {
                SyntaxClass::Normal(adjusted.kind(), adjusted)
            }
        }
        _ if expr.hash()
            || matches!(adjusted.kind(), SyntaxKind::MathIdent | SyntaxKind::Error) =>
        {
            SyntaxClass::Normal(adjusted.kind(), adjusted)
        }
        _ => return None,
    })
}

/// Whether the node might be in code trivia. This is a bit internal so please
/// check the caller to understand it.
fn possible_in_code_trivia(kind: SyntaxKind) -> bool {
    !matches!(
        interpret_mode_at_kind(kind),
        Some(InterpretMode::Markup | InterpretMode::Math | InterpretMode::Comment)
    )
}

/// Classes of arguments that can be operated on by IDE functionality.
#[derive(Debug, Clone)]
pub enum ArgClass<'a> {
    /// A positional argument.
    Positional {
        /// The spread arguments met before the positional argument.
        spreads: EcoVec<LinkedNode<'a>>,
        /// The index of the positional argument.
        positional: usize,
        /// Whether the positional argument is a spread argument.
        is_spread: bool,
    },
    /// A named argument.
    Named(LinkedNode<'a>),
}

impl ArgClass<'_> {
    /// Creates the class refer to the first positional argument.
    pub fn first_positional() -> Self {
        ArgClass::Positional {
            spreads: EcoVec::new(),
            positional: 0,
            is_spread: false,
        }
    }
}

// todo: whether we can merge `SurroundingSyntax` and `SyntaxContext`?
/// Classes of syntax context (outer syntax) that can be operated on by IDE
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, strum::EnumIter)]
pub enum SurroundingSyntax {
    /// Regular syntax.
    Regular,
    /// Content in a string.
    StringContent,
    /// The cursor is directly on the selector of a show rule.
    Selector,
    /// The cursor is directly on the transformation of a show rule.
    ShowTransform,
    /// The cursor is directly on the import list.
    ImportList,
    /// The cursor is directly on the set rule.
    SetRule,
    /// The cursor is directly on the parameter list.
    ParamList,
}

/// Determines the surrounding syntax of the node at the position.
pub fn surrounding_syntax(node: &LinkedNode) -> SurroundingSyntax {
    check_previous_syntax(node)
        .or_else(|| check_surrounding_syntax(node))
        .unwrap_or(SurroundingSyntax::Regular)
}

fn check_surrounding_syntax(mut leaf: &LinkedNode) -> Option<SurroundingSyntax> {
    use SurroundingSyntax::*;
    let mut met_args = false;

    if matches!(leaf.kind(), SyntaxKind::Str) {
        return Some(StringContent);
    }

    while let Some(parent) = leaf.parent() {
        crate::log_debug_ct!(
            "check_surrounding_syntax: {:?}::{:?}",
            parent.kind(),
            leaf.kind()
        );
        match parent.kind() {
            SyntaxKind::CodeBlock
            | SyntaxKind::ContentBlock
            | SyntaxKind::Equation
            | SyntaxKind::Closure => {
                return Some(Regular);
            }
            SyntaxKind::ImportItemPath
            | SyntaxKind::ImportItems
            | SyntaxKind::RenamedImportItem => {
                return Some(ImportList);
            }
            SyntaxKind::ModuleImport => {
                let colon = parent.children().find(|s| s.kind() == SyntaxKind::Colon);
                let Some(colon) = colon else {
                    return Some(Regular);
                };

                if leaf.offset() >= colon.offset() {
                    return Some(ImportList);
                } else {
                    return Some(Regular);
                }
            }
            SyntaxKind::Named => {
                let colon = parent.children().find(|s| s.kind() == SyntaxKind::Colon);
                let Some(colon) = colon else {
                    return Some(Regular);
                };

                return if leaf.offset() >= colon.offset() {
                    Some(Regular)
                } else if node_ancestors(leaf).any(|n| n.kind() == SyntaxKind::Params) {
                    Some(ParamList)
                } else {
                    Some(Regular)
                };
            }
            SyntaxKind::Params => {
                return Some(ParamList);
            }
            SyntaxKind::Args => {
                met_args = true;
            }
            SyntaxKind::SetRule => {
                let rule = parent.get().cast::<ast::SetRule>()?;
                if met_args || enclosed_by(parent, rule.condition().map(|s| s.span()), leaf) {
                    return Some(Regular);
                } else {
                    return Some(SetRule);
                }
            }
            SyntaxKind::ShowRule => {
                if met_args {
                    return Some(Regular);
                }

                let rule = parent.get().cast::<ast::ShowRule>()?;
                let colon = rule
                    .to_untyped()
                    .children()
                    .find(|s| s.kind() == SyntaxKind::Colon);
                let Some(colon) = colon.and_then(|colon| parent.find(colon.span())) else {
                    // incomplete show rule
                    return Some(Selector);
                };

                if leaf.offset() >= colon.offset() {
                    return Some(ShowTransform);
                } else {
                    return Some(Selector); // query's first argument
                }
            }
            _ => {}
        }

        leaf = parent;
    }

    None
}

fn check_previous_syntax(leaf: &LinkedNode) -> Option<SurroundingSyntax> {
    let mut leaf = leaf.clone();
    if leaf.kind().is_trivia() {
        leaf = leaf.prev_sibling()?;
    }
    if matches!(
        leaf.kind(),
        SyntaxKind::ShowRule
            | SyntaxKind::SetRule
            | SyntaxKind::ModuleImport
            | SyntaxKind::ModuleInclude
    ) {
        return check_surrounding_syntax(&leaf.rightmost_leaf()?);
    }

    if matches!(leaf.kind(), SyntaxKind::Show) {
        return Some(SurroundingSyntax::Selector);
    }
    if matches!(leaf.kind(), SyntaxKind::Set) {
        return Some(SurroundingSyntax::SetRule);
    }

    None
}

fn enclosed_by(parent: &LinkedNode, s: Option<Span>, leaf: &LinkedNode) -> bool {
    s.and_then(|s| parent.find(s)?.find(leaf.span())).is_some()
}

/// Classes of syntax context (outer syntax) that can be operated on by IDE
/// functionality.
///
/// A syntax context is either a [`SyntaxClass`] or other things.
/// One thing is not necessary to refer to some exact node. For example, a
/// cursor moving after some comma in a function call is identified as a
/// [`SyntaxContext::Arg`].
#[derive(Debug, Clone)]
pub enum SyntaxContext<'a> {
    /// A cursor on an argument.
    Arg {
        /// The callee node.
        callee: LinkedNode<'a>,
        /// The arguments node.
        args: LinkedNode<'a>,
        /// The argument target pointed by the cursor.
        target: ArgClass<'a>,
        /// Whether the callee is a set rule.
        is_set: bool,
    },
    /// A cursor on an element in an array or dictionary literal.
    Element {
        /// The container node.
        container: LinkedNode<'a>,
        /// The element target pointed by the cursor.
        target: ArgClass<'a>,
    },
    /// A cursor on a parenthesized expression.
    Paren {
        /// The parenthesized expression node.
        container: LinkedNode<'a>,
        /// Whether the cursor is on the left side of the parenthesized
        /// expression.
        is_before: bool,
    },
    /// A variable access expression.
    ///
    /// It can be either an identifier or a field access.
    VarAccess(VarClass<'a>),
    /// A cursor on an import path.
    ImportPath(LinkedNode<'a>),
    /// A cursor on an include path.
    IncludePath(LinkedNode<'a>),
    /// A cursor on a label.
    Label {
        /// The label node.
        node: LinkedNode<'a>,
        /// Whether the label is converted from an error node.
        is_error: bool,
    },
    /// A cursor on a normal [`SyntaxClass`].
    Normal(LinkedNode<'a>),
}

impl<'a> SyntaxContext<'a> {
    /// Gets the node of the cursor class.
    pub fn node(&self) -> Option<LinkedNode<'a>> {
        Some(match self {
            SyntaxContext::Arg { target, .. } | SyntaxContext::Element { target, .. } => {
                match target {
                    ArgClass::Positional { .. } => return None,
                    ArgClass::Named(node) => node.clone(),
                }
            }
            SyntaxContext::VarAccess(cls) => cls.node().clone(),
            SyntaxContext::Paren { container, .. } => container.clone(),
            SyntaxContext::Label { node, .. }
            | SyntaxContext::ImportPath(node)
            | SyntaxContext::IncludePath(node)
            | SyntaxContext::Normal(node) => node.clone(),
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

/// Classifies node's context (outer syntax) by outer node that can be operated
/// on by IDE functionality.
pub fn classify_context_outer<'a>(
    outer: LinkedNode<'a>,
    node: LinkedNode<'a>,
) -> Option<SyntaxContext<'a>> {
    use SyntaxClass::*;
    let context_syntax = classify_syntax(outer.clone(), node.offset())?;
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
            let arg_target = arg_context(args.clone(), node, ArgSourceKind::Call)?;
            Some(SyntaxContext::Arg {
                callee,
                args,
                target: arg_target,
                is_set,
            })
        }
        _ => None,
    }
}

/// Classifies node's context (outer syntax) that can be operated on by IDE
/// functionality.
pub fn classify_context(node: LinkedNode, cursor: Option<usize>) -> Option<SyntaxContext<'_>> {
    let mut node = node;
    if node.kind().is_trivia() && node.parent_kind().is_some_and(possible_in_code_trivia) {
        loop {
            node = node.prev_sibling()?;

            if !node.kind().is_trivia() {
                break;
            }
        }
    }

    let cursor = cursor.unwrap_or_else(|| node.offset());
    let syntax = classify_syntax(node.clone(), cursor)?;

    let normal_syntax = match syntax {
        SyntaxClass::Callee(callee) => {
            return callee_context(callee, node);
        }
        SyntaxClass::Label { node, is_error } => {
            return Some(SyntaxContext::Label { node, is_error });
        }
        SyntaxClass::ImportPath(node) => {
            return Some(SyntaxContext::ImportPath(node));
        }
        SyntaxClass::IncludePath(node) => {
            return Some(SyntaxContext::IncludePath(node));
        }
        syntax => syntax,
    };

    let Some(mut node_parent) = node.parent().cloned() else {
        return Some(SyntaxContext::Normal(node));
    };

    while let SyntaxKind::Named | SyntaxKind::Colon = node_parent.kind() {
        let Some(parent) = node_parent.parent() else {
            return Some(SyntaxContext::Normal(node));
        };
        node_parent = parent.clone();
    }

    match node_parent.kind() {
        SyntaxKind::Args => {
            let callee = node_ancestors(&node_parent).find_map(|ancestor| {
                let span = match ancestor.cast::<ast::Expr>()? {
                    ast::Expr::FuncCall(call) => call.callee().span(),
                    ast::Expr::Set(set) => set.target().span(),
                    _ => return None,
                };
                ancestor.find(span)
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

            callee_context(callee, param_node)
        }
        SyntaxKind::Array | SyntaxKind::Dict => {
            let element_target = arg_context(
                node_parent.clone(),
                node.clone(),
                match node_parent.kind() {
                    SyntaxKind::Array => ArgSourceKind::Array,
                    SyntaxKind::Dict => ArgSourceKind::Dict,
                    _ => unreachable!(),
                },
            )?;
            Some(SyntaxContext::Element {
                container: node_parent.clone(),
                target: element_target,
            })
        }
        SyntaxKind::Parenthesized => {
            let is_before = node.offset() <= node_parent.offset() + 1;
            Some(SyntaxContext::Paren {
                container: node_parent.clone(),
                is_before,
            })
        }
        _ => Some(match normal_syntax {
            SyntaxClass::VarAccess(v) => SyntaxContext::VarAccess(v),
            normal_syntax => SyntaxContext::Normal(normal_syntax.node().clone()),
        }),
    }
}

fn callee_context<'a>(callee: LinkedNode<'a>, node: LinkedNode<'a>) -> Option<SyntaxContext<'a>> {
    let parent = callee.parent()?;
    let args = match parent.cast::<ast::Expr>() {
        Some(ast::Expr::FuncCall(call)) => call.args(),
        Some(ast::Expr::Set(set)) => set.args(),
        _ => return None,
    };
    let args = parent.find(args.span())?;

    let is_set = parent.kind() == SyntaxKind::SetRule;
    let target = arg_context(args.clone(), node, ArgSourceKind::Call)?;
    Some(SyntaxContext::Arg {
        callee,
        args,
        target,
        is_set,
    })
}

fn arg_context<'a>(
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
    use typst::syntax::{is_newline, Side, Source};

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
            let node = root.leaf_at(cursor, Side::Before);
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

    fn map_context(source: &str) -> String {
        map_node(source, |root, cursor| {
            let node = root.leaf_at(cursor, Side::Before);
            let kind = node.and_then(|node| classify_context(node, Some(cursor)));
            match kind {
                Some(SyntaxContext::Arg { .. }) => 'p',
                Some(SyntaxContext::Element { .. }) => 'e',
                Some(SyntaxContext::Paren { .. }) => 'P',
                Some(SyntaxContext::VarAccess { .. }) => 'v',
                Some(SyntaxContext::ImportPath(..)) => 'i',
                Some(SyntaxContext::IncludePath(..)) => 'I',
                Some(SyntaxContext::Label { .. }) => 'l',
                Some(SyntaxContext::Normal(..)) => 'n',
                None => ' ',
            }
        })
    }

    #[test]
    fn test_get_syntax() {
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
    fn test_get_context() {
        assert_snapshot!(map_context(r#"#let x = 1  
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
        assert_snapshot!(map_context(r#"#let f(x);"#).trim(), @r"
        #let f(x);
         nnnnv v
        ");
        assert_snapshot!(map_context(r#"#f(1, 2)   Test"#).trim(), @r"
        #f(1, 2)   Test
         vpppppp
        ");
        assert_snapshot!(map_context(r#"#()   Test"#).trim(), @r"
        #()   Test
         ee
        ");
        assert_snapshot!(map_context(r#"#(1)   Test"#).trim(), @r"
        #(1)   Test
         PPP
        ");
        assert_snapshot!(map_context(r#"#(a: 1)   Test"#).trim(), @r"
        #(a: 1)   Test
         eeeeee
        ");
        assert_snapshot!(map_context(r#"#(1, 2)   Test"#).trim(), @r"
        #(1, 2)   Test
         eeeeee
        ");
        assert_snapshot!(map_context(r#"#(1, 2)  
  Test"#).trim(), @r"
        #(1, 2)  
         eeeeee  
          Test
        ");
    }

    fn access_node(s: &str, cursor: i32) -> String {
        access_node_(s, cursor).unwrap_or_default()
    }

    fn access_node_(s: &str, cursor: i32) -> Option<String> {
        access_var(s, cursor, |_source, var| {
            Some(var.accessed_node()?.get().clone().into_text().into())
        })
    }

    fn access_field(s: &str, cursor: i32) -> String {
        access_field_(s, cursor).unwrap_or_default()
    }

    fn access_field_(s: &str, cursor: i32) -> Option<String> {
        access_var(s, cursor, |source, var| {
            let field = var.accessing_field()?;
            Some(match field {
                FieldClass::Field(ident) => format!("Field: {}", ident.text()),
                FieldClass::DotSuffix(span_offset) => {
                    let offset = source.find(span_offset.span)?.offset() + span_offset.offset;
                    format!("DotSuffix: {offset:?}")
                }
            })
        })
    }

    fn access_var(
        s: &str,
        cursor: i32,
        f: impl FnOnce(&Source, VarClass) -> Option<String>,
    ) -> Option<String> {
        let cursor = if cursor < 0 {
            s.len() as i32 + cursor
        } else {
            cursor
        };
        let source = Source::detached(s.to_owned());
        let root = LinkedNode::new(source.root());
        let node = root.leaf_at(cursor as usize, Side::Before)?;
        let syntax = classify_syntax(node, cursor as usize)?;
        let SyntaxClass::VarAccess(var) = syntax else {
            return None;
        };
        f(&source, var)
    }

    #[test]
    fn test_access_field() {
        assert_snapshot!(access_field("#(a.b)", 5), @r"Field: b");
        assert_snapshot!(access_field("#a.", 3), @"DotSuffix: 3");
        assert_snapshot!(access_field("$a.$", 3), @"DotSuffix: 3");
        assert_snapshot!(access_field("#(a.)", 4), @"DotSuffix: 4");
        assert_snapshot!(access_node("#(a..b)", 4), @"a");
        assert_snapshot!(access_field("#(a..b)", 4), @"DotSuffix: 4");
        assert_snapshot!(access_node("#(a..b())", 4), @"a");
        assert_snapshot!(access_field("#(a..b())", 4), @"DotSuffix: 4");
    }

    #[test]
    fn test_code_access() {
        assert_snapshot!(access_node("#{`a`.}", 6), @"`a`");
        assert_snapshot!(access_field("#{`a`.}", 6), @"DotSuffix: 6");
        assert_snapshot!(access_node("#{$a$.}", 6), @"$a$");
        assert_snapshot!(access_field("#{$a$.}", 6), @"DotSuffix: 6");
        assert_snapshot!(access_node("#{\"a\".}", 6), @"\"a\"");
        assert_snapshot!(access_field("#{\"a\".}", 6), @"DotSuffix: 6");
        assert_snapshot!(access_node("#{<a>.}", 6), @"<a>");
        assert_snapshot!(access_field("#{<a>.}", 6), @"DotSuffix: 6");
    }

    #[test]
    fn test_markup_access() {
        assert_snapshot!(access_field("_a_.", 4), @"");
        assert_snapshot!(access_field("*a*.", 4), @"");
        assert_snapshot!(access_field("`a`.", 4), @"");
        assert_snapshot!(access_field("$a$.", 4), @"");
        assert_snapshot!(access_field("\"a\".", 4), @"");
        assert_snapshot!(access_field("@a.", 3), @"");
        assert_snapshot!(access_field("<a>.", 4), @"");
    }

    #[test]
    fn test_hash_access() {
        assert_snapshot!(access_node("#a.", 3), @"a");
        assert_snapshot!(access_field("#a.", 3), @"DotSuffix: 3");
        assert_snapshot!(access_node("#(a).", 5), @"(a)");
        assert_snapshot!(access_field("#(a).", 5), @"DotSuffix: 5");
        assert_snapshot!(access_node("#`a`.", 5), @"`a`");
        assert_snapshot!(access_field("#`a`.", 5), @"DotSuffix: 5");
        assert_snapshot!(access_node("#$a$.", 5), @"$a$");
        assert_snapshot!(access_field("#$a$.", 5), @"DotSuffix: 5");
        assert_snapshot!(access_node("#(a,).", 6), @"(a,)");
        assert_snapshot!(access_field("#(a,).", 6), @"DotSuffix: 6");
    }
}
