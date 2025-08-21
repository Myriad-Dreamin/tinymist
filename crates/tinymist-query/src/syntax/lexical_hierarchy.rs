use std::ops::{Deref, Range};

use ecow::{EcoString, EcoVec, eco_vec};
use lsp_types::SymbolKind;
use serde::{Deserialize, Serialize};
use typst::syntax::{
    LinkedNode, Source, SyntaxKind,
    ast::{self},
};
use typst_shim::utils::LazyHash;

use super::{CommentGroupMatcher, is_mark};

/// Extracts the lexical hierarchy from a Typst source file.
///
/// Analyzes the source code to build a hierarchical structure of symbols, headings,
/// and other lexical elements that can be used for document outline, symbol navigation,
/// and other language server features.
#[typst_macros::time(span = source.root().span())]
pub(crate) fn get_lexical_hierarchy(
    source: &Source,
    scope_kind: LexicalScopeKind,
) -> Option<EcoVec<LexicalHierarchy>> {
    let start = tinymist_std::time::Instant::now();
    let root = LinkedNode::new(source.root());

    let mut worker = LexicalHierarchyWorker {
        sk: scope_kind,
        ..LexicalHierarchyWorker::default()
    };
    worker.stack.push((
        LexicalInfo {
            name: "deadbeef".into(),
            kind: LexicalKind::Heading(-1),
            range: 0..0,
        },
        eco_vec![],
    ));
    let res = match worker.check_node(root) {
        Some(()) => Some(()),
        None => {
            log::error!("lexical hierarchy analysis failed");
            None
        }
    };

    while worker.stack.len() > 1 {
        worker.finish_hierarchy();
    }

    crate::log_debug_ct!("lexical hierarchy analysis took {:?}", start.elapsed());
    res.map(|_| worker.stack.pop().unwrap().1)
}

/// Represents different kinds of variable-like lexical elements in Typst source code.
///
/// Distinguishes between various forms of identifiers and references that can appear
/// in Typst syntax, each with different semantic meanings and scoping rules.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum LexicalVarKind {
    /// `#foo`
    ///   ^^^
    ValRef,
    /// `@foo`
    ///   ^^^
    LabelRef,
    /// `<foo>`
    ///   ^^^
    Label,
    /// `x:`
    ///  ^^
    BibKey,
    /// `let foo`
    ///      ^^^
    Variable,
    /// `let foo()`
    ///      ^^^
    Function,
}

/// Represents the different categories of lexical elements in Typst source code.
///
/// Defines the main types of symbols and structural elements that can be identified
/// during lexical analysis, including headings, variables, blocks, and comment groups.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum LexicalKind {
    Heading(i16),
    Var(LexicalVarKind),
    Block,
    CommentGroup,
}

impl LexicalKind {
    /// Creates a label lexical kind.
    const fn label() -> LexicalKind {
        LexicalKind::Var(LexicalVarKind::Label)
    }

    /// Creates a function lexical kind.
    const fn function() -> LexicalKind {
        LexicalKind::Var(LexicalVarKind::Function)
    }

    /// Creates a variable lexical kind.
    const fn variable() -> LexicalKind {
        LexicalKind::Var(LexicalVarKind::Variable)
    }

    /// Determines if this lexical kind represents a valid LSP symbol.
    ///
    /// Returns `false` for blocks and comment groups, which are structural
    /// elements but not meaningful symbols for language server features.
    pub fn is_valid_lsp_symbol(&self) -> bool {
        !matches!(self, LexicalKind::Block | LexicalKind::CommentGroup)
    }
}

impl From<LexicalKind> for SymbolKind {
    fn from(value: LexicalKind) -> Self {
        use LexicalVarKind::*;
        match value {
            LexicalKind::Heading(..) => SymbolKind::NAMESPACE,
            LexicalKind::Var(ValRef | Variable) => SymbolKind::VARIABLE,
            LexicalKind::Var(Function) => SymbolKind::FUNCTION,
            LexicalKind::Var(LabelRef | Label | BibKey) => SymbolKind::CONSTANT,
            LexicalKind::Block | LexicalKind::CommentGroup => SymbolKind::CONSTANT,
        }
    }
}

/// Defines the scope behavior for lexical hierarchy analysis.
///
/// Controls which types of lexical elements are analyzed and included
/// in the hierarchy based on the intended use case.
#[derive(Debug, Clone, Copy, Hash, Default, PartialEq, Eq)]
pub(crate) enum LexicalScopeKind {
    #[default]
    Symbol,
    Braced,
}

impl LexicalScopeKind {
    /// Checks if this scope kind affects symbol analysis.
    fn affect_symbol(&self) -> bool {
        matches!(self, Self::Symbol)
    }

    /// Checks if this scope kind affects markup analysis.
    fn affect_markup(&self) -> bool {
        matches!(self, Self::Braced)
    }

    /// Checks if this scope kind affects block analysis.
    fn affect_block(&self) -> bool {
        matches!(self, Self::Braced)
    }

    /// Checks if this scope kind affects expression analysis.
    fn affect_expr(&self) -> bool {
        matches!(self, Self::Braced)
    }

    /// Checks if this scope kind affects heading analysis.
    fn affect_heading(&self) -> bool {
        matches!(self, Self::Symbol | Self::Braced)
    }
}

/// Contains information about a lexical element in the source code.
///
/// Stores the essential metadata for a symbol or structural element,
/// including its name, type, and location in the source.
#[derive(Debug, Clone, Hash)]
pub(crate) struct LexicalInfo {
    pub name: EcoString,
    pub kind: LexicalKind,
    pub range: Range<usize>,
}

/// Represents a node in the lexical hierarchy tree.
///
/// Contains lexical information for a single element and optionally
/// references to its child elements, forming a hierarchical structure
/// of the source code's lexical organization.
#[derive(Debug, Clone, Hash)]
pub(crate) struct LexicalHierarchy {
    pub info: LexicalInfo,
    pub children: Option<LazyHash<EcoVec<LexicalHierarchy>>>,
}

/// Provides custom serialization for `LexicalHierarchy`.
///
/// Serializes the hierarchy structure while handling the optional children
/// field appropriately for external consumers.
impl Serialize for LexicalHierarchy {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("LexicalHierarchy", 2)?;
        state.serialize_field("name", &self.info.name)?;
        state.serialize_field("kind", &self.info.kind)?;
        state.serialize_field("range", &self.info.range)?;
        if let Some(children) = &self.children {
            state.serialize_field("children", children.deref())?;
        }
        state.end()
    }
}

/// Provides custom deserialization for `LexicalHierarchy`.
///
/// Reconstructs the hierarchy structure from serialized data, properly
/// handling the optional children field and ensuring data integrity.
impl<'de> Deserialize<'de> for LexicalHierarchy {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::MapAccess;
        struct LexicalHierarchyVisitor;
        impl<'de> serde::de::Visitor<'de> for LexicalHierarchyVisitor {
            type Value = LexicalHierarchy;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a struct")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut name = None;
                let mut kind = None;
                let mut range = None;
                let mut children = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        "name" => name = Some(map.next_value()?),
                        "kind" => kind = Some(map.next_value()?),
                        "range" => range = Some(map.next_value()?),
                        "children" => children = Some(map.next_value()?),
                        _ => {}
                    }
                }
                let name = name.ok_or_else(|| serde::de::Error::missing_field("name"))?;
                let kind = kind.ok_or_else(|| serde::de::Error::missing_field("kind"))?;
                let range = range.ok_or_else(|| serde::de::Error::missing_field("range"))?;
                Ok(LexicalHierarchy {
                    info: LexicalInfo { name, kind, range },
                    children: children.map(LazyHash::new),
                })
            }
        }

        deserializer.deserialize_struct(
            "LexicalHierarchy",
            &["name", "kind", "range", "children"],
            LexicalHierarchyVisitor,
        )
    }
}

/// Tracks the context for identifier analysis during hierarchy building.
///
/// Determines how identifiers should be interpreted based on their
/// syntactic context within the source code structure.
#[derive(Debug, Clone, Copy, Hash, Default, PartialEq, Eq)]
enum IdentContext {
    #[default]
    Ref,
    Func,
    Var,
    Params,
}

/// Manages the state and operations for building lexical hierarchies.
///
/// Maintains a stack-based approach to track nested scopes and contexts
/// while traversing the syntax tree to build the hierarchical structure.
#[derive(Default)]
struct LexicalHierarchyWorker {
    sk: LexicalScopeKind,
    stack: Vec<(LexicalInfo, EcoVec<LexicalHierarchy>)>,
    ident_context: IdentContext,
}

impl LexicalHierarchyWorker {
    /// Checks if a syntax kind represents a plain token that doesn't need analysis.
    ///
    /// Returns `true` for trivia, keywords, marks, and error tokens that
    /// don't contribute to the lexical hierarchy structure.
    fn is_plain_token(kind: SyntaxKind) -> bool {
        kind.is_trivia() || kind.is_keyword() || is_mark(kind) || kind.is_error()
    }

    /// Completes the current hierarchy level and adds it to the parent.
    ///
    /// Pops the current symbol and its children from the stack, creates
    /// a hierarchy node, and adds it to the parent level.
    fn finish_hierarchy(&mut self) {
        let (symbol, children) = self.stack.pop().unwrap();
        let current = &mut self.stack.last_mut().unwrap().1;

        current.push(finish_hierarchy(symbol, children));
    }

    /// Enters a syntax node and updates the identifier context.
    ///
    /// Adjusts the current identifier context based on the node type,
    /// enabling proper interpretation of identifiers within different
    /// syntactic structures. Returns the previous context for restoration.
    fn enter_node(&mut self, node: &LinkedNode) -> Option<IdentContext> {
        let checkpoint = self.ident_context;
        match node.kind() {
            SyntaxKind::RefMarker => self.ident_context = IdentContext::Ref,
            SyntaxKind::LetBinding => self.ident_context = IdentContext::Ref,
            SyntaxKind::Closure => self.ident_context = IdentContext::Func,
            SyntaxKind::Params => self.ident_context = IdentContext::Params,
            _ => {}
        }

        Some(checkpoint)
    }

    /// Exits a syntax node and restores the previous identifier context.
    ///
    /// Restores the identifier context to its state before entering
    /// the current node, maintaining proper context tracking.
    fn exit_node(&mut self, checkpoint: IdentContext) -> Option<()> {
        self.ident_context = checkpoint;
        Some(())
    }

    /// Processes child nodes recursively with comment group detection.
    ///
    /// Analyzes all child nodes of a given node, handling comment groups
    /// specially and recursively processing other significant elements.
    fn check_nodes(&mut self, node: LinkedNode) -> Option<()> {
        let mut group_matcher = CommentGroupMatcher::default();
        let mut comment_range: Option<Range<usize>> = None;
        for child in node.children() {
            match group_matcher.process(&child) {
                super::CommentGroupSignal::Space => {}
                super::CommentGroupSignal::LineComment
                | super::CommentGroupSignal::BlockComment => {
                    let child_range = child.range();
                    match comment_range {
                        Some(ref mut comment_range) => comment_range.end = child_range.end,
                        None => comment_range = Some(child_range),
                    }
                }
                super::CommentGroupSignal::Hash | super::CommentGroupSignal::BreakGroup => {
                    if let Some(comment_range) = comment_range.take() {
                        self.stack.push((
                            LexicalInfo {
                                name: "".into(),
                                kind: LexicalKind::CommentGroup,
                                range: comment_range,
                            },
                            eco_vec![],
                        ));
                    }

                    if !Self::is_plain_token(child.kind()) {
                        self.check_node(child)?;
                    }
                }
            }
        }

        Some(())
    }

    /// Analyzes a syntax node and builds its lexical hierarchy.
    ///
    /// Processes a single node to extract lexical information and recursively
    /// analyze its children, handling special cases for different node types
    /// and maintaining proper hierarchy structure.
    fn check_node(&mut self, node: LinkedNode) -> Option<()> {
        let own_symbol = self.get_ident(&node)?;

        let checkpoint = self.enter_node(&node)?;

        if let Some(symbol) = own_symbol {
            if let LexicalKind::Heading(level) = symbol.kind {
                'heading_break: while let Some((w, _)) = self.stack.last() {
                    match w.kind {
                        LexicalKind::Heading(lvl) if lvl < level => break 'heading_break,
                        LexicalKind::Block => break 'heading_break,
                        _ if self.stack.len() <= 1 => break 'heading_break,
                        _ => {}
                    }

                    self.finish_hierarchy();
                }
            }
            let is_heading = matches!(symbol.kind, LexicalKind::Heading(..));

            self.stack.push((symbol, eco_vec![]));
            let stack_height = self.stack.len();

            if node.kind() != SyntaxKind::ModuleImport {
                self.check_nodes(node)?;
            }

            if is_heading {
                while stack_height < self.stack.len() {
                    self.finish_hierarchy();
                }
            } else {
                while stack_height <= self.stack.len() {
                    self.finish_hierarchy();
                }
            }
        } else {
            // todo: for loop variable
            match node.kind() {
                SyntaxKind::LetBinding => 'let_binding: {
                    let pattern = node.children().find(|n| n.cast::<ast::Pattern>().is_some());

                    if let Some(name) = &pattern {
                        let pat = name.cast::<ast::Pattern>().unwrap();

                        // special case: it will then match SyntaxKind::Closure in the inner looking
                        // up.
                        if matches!(pat, ast::Pattern::Normal(ast::Expr::Closure(..))) {
                            let closure = name.clone();
                            self.check_node_with(closure, IdentContext::Ref)?;
                            break 'let_binding;
                        }
                    }

                    // reverse order for correct symbol affection
                    let name_offset = pattern.as_ref().map(|node| node.offset());
                    self.check_opt_node_with(pattern, IdentContext::Var)?;
                    self.check_first_sub_expr(node.children().rev(), name_offset)?;
                }
                SyntaxKind::ForLoop => {
                    let pattern = node.children().find(|child| child.is::<ast::Pattern>());
                    let iterable = node
                        .children()
                        .skip_while(|child| child.kind() != SyntaxKind::In)
                        .find(|child| child.is::<ast::Expr>());

                    let iterable_offset = iterable.as_ref().map(|node| node.offset());
                    self.check_opt_node_with(iterable, IdentContext::Ref)?;
                    self.check_opt_node_with(pattern, IdentContext::Var)?;
                    self.check_first_sub_expr(node.children().rev(), iterable_offset)?;
                }
                SyntaxKind::Closure => {
                    let first_child = node.children().next();
                    let current = self.stack.last_mut().unwrap().1.len();
                    if let Some(first_child) = first_child {
                        if first_child.kind() == SyntaxKind::Ident {
                            self.check_node_with(first_child, IdentContext::Func)?;
                        }
                    }
                    let body = node
                        .children()
                        .rev()
                        .find(|child| child.cast::<ast::Expr>().is_some());
                    if let Some(body) = body {
                        let symbol = if current == self.stack.last().unwrap().1.len() {
                            // Closure has no updated symbol stack
                            LexicalInfo {
                                name: "<anonymous>".into(),
                                kind: LexicalKind::function(),
                                range: node.range(),
                            }
                        } else {
                            // Closure has a name
                            let mut info = self.stack.last_mut().unwrap().1.pop().unwrap().info;
                            info.range = node.range();
                            info
                        };

                        self.stack.push((symbol, eco_vec![]));
                        let stack_height = self.stack.len();

                        self.check_node_with(body, IdentContext::Ref)?;
                        while stack_height <= self.stack.len() {
                            self.finish_hierarchy();
                        }
                    }
                }
                SyntaxKind::FieldAccess => {
                    self.check_first_sub_expr(node.children(), None)?;
                }
                SyntaxKind::Named => {
                    self.check_first_sub_expr(node.children().rev(), None)?;

                    if self.ident_context == IdentContext::Params {
                        let ident = node.children().find(|n| n.kind() == SyntaxKind::Ident);
                        self.check_opt_node_with(ident, IdentContext::Var)?;
                    }
                }
                kind if Self::is_plain_token(kind) => {}
                _ => {
                    self.check_nodes(node)?;
                }
            }
        }

        self.exit_node(checkpoint)?;

        Some(())
    }

    /// Processes an optional node with a specific identifier context.
    ///
    /// Convenience method that applies a specific context to a node
    /// if it exists, otherwise does nothing.
    #[inline(always)]
    fn check_opt_node_with(
        &mut self,
        node: Option<LinkedNode>,
        context: IdentContext,
    ) -> Option<()> {
        if let Some(node) = node {
            self.check_node_with(node, context)?;
        }

        Some(())
    }

    /// Finds and processes the first sub-expression in a node sequence.
    ///
    /// Locates the first expression node in the given iterator and processes
    /// it, optionally checking that it appears after a specified offset.
    /// Used for handling expression order in complex syntax structures.
    fn check_first_sub_expr<'a>(
        &mut self,
        mut nodes: impl Iterator<Item = LinkedNode<'a>>,
        after_offset: Option<usize>,
    ) -> Option<()> {
        let body = nodes.find(|n| n.is::<ast::Expr>());
        if let Some(body) = body {
            if after_offset.is_some_and(|offset| offset >= body.offset()) {
                return Some(());
            }
            self.check_node_with(body, IdentContext::Ref)?;
        }

        Some(())
    }

    /// Processes a node with a temporarily modified identifier context.
    ///
    /// Changes the identifier context for processing a specific node,
    /// then restores the original context afterward.
    fn check_node_with(&mut self, node: LinkedNode, context: IdentContext) -> Option<()> {
        let parent_context = self.ident_context;
        self.ident_context = context;

        let res = self.check_node(node);

        self.ident_context = parent_context;
        res
    }

    /// Extracts lexical information from a syntax node.
    ///
    /// Analyzes a node to determine if it represents a meaningful lexical
    /// element and extracts its name, type, and range. Returns `None` for
    /// nodes that don't contribute to the lexical hierarchy.
    #[allow(deprecated)]
    fn get_ident(&self, node: &LinkedNode) -> Option<Option<LexicalInfo>> {
        let (name, kind) = match node.kind() {
            SyntaxKind::Label if self.sk.affect_symbol() => {
                // filter out label in code context.
                let prev_kind = node.prev_sibling_kind();
                if prev_kind.is_some_and(|prev_kind| {
                    matches!(
                        prev_kind,
                        SyntaxKind::LeftBracket
                            | SyntaxKind::LeftBrace
                            | SyntaxKind::LeftParen
                            | SyntaxKind::Comma
                            | SyntaxKind::Colon
                    ) || prev_kind.is_keyword()
                }) {
                    return Some(None);
                }
                let ast_node = node.cast::<ast::Label>()?;
                let name = ast_node.get().into();

                (name, LexicalKind::label())
            }
            SyntaxKind::Ident if self.sk.affect_symbol() => {
                let ast_node = node.cast::<ast::Ident>()?;
                let name = ast_node.get().clone();
                let kind = match self.ident_context {
                    IdentContext::Func => LexicalKind::function(),
                    IdentContext::Var | IdentContext::Params => LexicalKind::variable(),
                    _ => return Some(None),
                };

                (name, kind)
            }
            SyntaxKind::Equation | SyntaxKind::Raw | SyntaxKind::BlockComment
                if self.sk.affect_markup() =>
            {
                (EcoString::new(), LexicalKind::Block)
            }
            SyntaxKind::CodeBlock | SyntaxKind::ContentBlock if self.sk.affect_block() => {
                (EcoString::new(), LexicalKind::Block)
            }
            SyntaxKind::Parenthesized
            | SyntaxKind::Destructuring
            | SyntaxKind::Args
            | SyntaxKind::Array
            | SyntaxKind::Dict
                if self.sk.affect_expr() =>
            {
                (EcoString::new(), LexicalKind::Block)
            }
            SyntaxKind::Markup => {
                let name = node.get().to_owned().into_text();
                if name.is_empty() {
                    return Some(None);
                }
                let Some(parent) = node.parent() else {
                    return Some(None);
                };
                let kind = match parent.kind() {
                    SyntaxKind::Heading if self.sk.affect_heading() => LexicalKind::Heading(
                        parent.cast::<ast::Heading>().unwrap().depth().get() as i16,
                    ),
                    _ => return Some(None),
                };

                (name, kind)
            }
            SyntaxKind::ListItem => (EcoString::new(), LexicalKind::Block),
            SyntaxKind::EnumItem => (EcoString::new(), LexicalKind::Block),
            _ => return Some(None),
        };

        Some(Some(LexicalInfo {
            name,
            kind,
            range: node.range(),
        }))
    }
}

/// Creates a hierarchy node from lexical information and child nodes.
///
/// Constructs a `LexicalHierarchy` from the provided symbol information
/// and children, handling the case where no children exist.
fn finish_hierarchy(sym: LexicalInfo, curr: EcoVec<LexicalHierarchy>) -> LexicalHierarchy {
    LexicalHierarchy {
        info: sym,
        children: if curr.is_empty() {
            None
        } else {
            Some(LazyHash::new(curr))
        },
    }
}
