use std::ops::{Deref, Range};

use ecow::{eco_vec, EcoString, EcoVec};
use lsp_types::SymbolKind;
use serde::{Deserialize, Serialize};
use typst::syntax::{
    ast::{self},
    LinkedNode, Source, SyntaxKind,
};
use typst_shim::utils::LazyHash;

use super::{is_mark, CommentGroupMatcher};

pub(crate) fn get_lexical_hierarchy(
    source: &Source,
    scope_kind: LexicalScopeKind,
) -> Option<EcoVec<LexicalHierarchy>> {
    let start = std::time::Instant::now();
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

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum LexicalKind {
    Heading(i16),
    Var(LexicalVarKind),
    Block,
    CommentGroup,
}

impl LexicalKind {
    const fn label() -> LexicalKind {
        LexicalKind::Var(LexicalVarKind::Label)
    }

    const fn function() -> LexicalKind {
        LexicalKind::Var(LexicalVarKind::Function)
    }

    const fn variable() -> LexicalKind {
        LexicalKind::Var(LexicalVarKind::Variable)
    }
}

impl TryFrom<LexicalKind> for SymbolKind {
    type Error = ();

    fn try_from(value: LexicalKind) -> Result<Self, Self::Error> {
        match value {
            LexicalKind::Heading(..) => Ok(SymbolKind::NAMESPACE),
            LexicalKind::Var(LexicalVarKind::Variable) => Ok(SymbolKind::VARIABLE),
            LexicalKind::Var(LexicalVarKind::Function) => Ok(SymbolKind::FUNCTION),
            LexicalKind::Var(LexicalVarKind::Label) => Ok(SymbolKind::CONSTANT),
            LexicalKind::Var(..) | LexicalKind::Block | LexicalKind::CommentGroup => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, Hash, Default, PartialEq, Eq)]
pub(crate) enum LexicalScopeKind {
    #[default]
    Symbol,
    Braced,
}

impl LexicalScopeKind {
    fn affect_symbol(&self) -> bool {
        matches!(self, Self::Symbol)
    }

    fn affect_markup(&self) -> bool {
        matches!(self, Self::Braced)
    }

    fn affect_block(&self) -> bool {
        matches!(self, Self::Braced)
    }

    fn affect_expr(&self) -> bool {
        matches!(self, Self::Braced)
    }

    fn affect_heading(&self) -> bool {
        matches!(self, Self::Symbol | Self::Braced)
    }
}

#[derive(Debug, Clone, Hash)]
pub(crate) struct LexicalInfo {
    pub name: EcoString,
    pub kind: LexicalKind,
    pub range: Range<usize>,
}

#[derive(Debug, Clone, Hash)]
pub(crate) struct LexicalHierarchy {
    pub info: LexicalInfo,
    pub children: Option<LazyHash<EcoVec<LexicalHierarchy>>>,
}

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

#[derive(Debug, Clone, Copy, Hash, Default, PartialEq, Eq)]
enum IdentContext {
    #[default]
    Ref,
    Func,
    Var,
    Params,
}

#[derive(Default)]
struct LexicalHierarchyWorker {
    sk: LexicalScopeKind,
    stack: Vec<(LexicalInfo, EcoVec<LexicalHierarchy>)>,
    ident_context: IdentContext,
}

impl LexicalHierarchyWorker {
    fn is_plain_token(kind: SyntaxKind) -> bool {
        kind.is_trivia() || kind.is_keyword() || is_mark(kind) || kind.is_error()
    }

    /// Finish the current top of the stack.
    fn finish_hierarchy(&mut self) {
        let (symbol, children) = self.stack.pop().unwrap();
        let current = &mut self.stack.last_mut().unwrap().1;

        current.push(finish_hierarchy(symbol, children));
    }

    /// Enter a node and setup the context.
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

    /// Exit a node and restore the context.
    fn exit_node(&mut self, checkpoint: IdentContext) -> Option<()> {
        self.ident_context = checkpoint;
        Some(())
    }

    /// Check nodes in a list recursively.
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

    /// Check lexical hierarchy a node recursively.
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

    /// Check a possible node with a specific context.
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

    /// Check the first sub-expression of a node. If an offset is provided, it
    /// only checks the sub-expression if it starts after the offset.
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

    /// Check a node with a specific context.
    fn check_node_with(&mut self, node: LinkedNode, context: IdentContext) -> Option<()> {
        let parent_context = self.ident_context;
        self.ident_context = context;

        let res = self.check_node(node);

        self.ident_context = parent_context;
        res
    }

    /// Get symbol for a leaf node of a valid type, or `None` if the node is an
    /// invalid type.
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
