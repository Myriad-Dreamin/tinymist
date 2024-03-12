use std::ops::{Deref, Range};

use anyhow::anyhow;
use log::info;
use lsp_types::SymbolKind;
use serde::{Deserialize, Serialize};
use typst::{
    syntax::{ast, LinkedNode, Source, SyntaxKind},
    util::LazyHash,
};
use typst_ts_core::typst::prelude::{eco_vec, EcoVec};

pub(crate) fn get_lexical_hierarchy(
    source: Source,
    g: LexicalScopeKind,
) -> Option<EcoVec<LexicalHierarchy>> {
    let b = std::time::Instant::now();
    let root = LinkedNode::new(source.root());

    let mut worker = LexicalHierarchyWorker {
        g,
        ..LexicalHierarchyWorker::default()
    };
    worker.stack.push((
        LexicalInfo {
            name: "deadbeef".to_string(),
            kind: LexicalKind::Namespace(-1),
            range: 0..0,
        },
        eco_vec![],
    ));
    let res = worker.get_symbols(root).ok();

    while worker.stack.len() > 1 {
        worker.symbreak();
    }

    let e = std::time::Instant::now();
    info!("lexical hierarchy analysis took {:?}", e - b);
    res.map(|_| worker.stack.pop().unwrap().1)
}

#[derive(Debug, Clone, Copy, Hash, Serialize, Deserialize)]
pub(crate) enum LexicalKind {
    Namespace(i16),
    ValRef,
    LabelRef,
    Variable,
    Function,
    Label,
    Block,
}

impl TryFrom<LexicalKind> for SymbolKind {
    type Error = ();

    fn try_from(value: LexicalKind) -> Result<Self, Self::Error> {
        match value {
            LexicalKind::Namespace(..) => Ok(SymbolKind::NAMESPACE),
            LexicalKind::Variable => Ok(SymbolKind::VARIABLE),
            LexicalKind::Function => Ok(SymbolKind::FUNCTION),
            LexicalKind::Label => Ok(SymbolKind::CONSTANT),
            LexicalKind::ValRef | LexicalKind::LabelRef | LexicalKind::Block => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, Hash, Default, PartialEq, Eq)]
pub(crate) enum LexicalScopeKind {
    #[default]
    Symbol,
    Braced,
    DefUse,
}

impl LexicalScopeKind {
    fn affect_symbol(&self) -> bool {
        matches!(self, Self::DefUse | Self::Symbol)
    }

    fn affect_ref(&self) -> bool {
        matches!(self, Self::DefUse)
    }

    fn affect_markup(&self) -> bool {
        matches!(self, Self::Braced)
    }

    fn affect_block(&self) -> bool {
        matches!(self, Self::DefUse | Self::Braced)
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
    pub name: String,
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
    g: LexicalScopeKind,
    stack: Vec<(LexicalInfo, EcoVec<LexicalHierarchy>)>,
    ident_context: IdentContext,
}

impl LexicalHierarchyWorker {
    fn symbreak(&mut self) {
        let (symbol, children) = self.stack.pop().unwrap();
        let current = &mut self.stack.last_mut().unwrap().1;

        current.push(symbreak(symbol, children));
    }

    fn enter_symbol_context(&mut self, node: &LinkedNode) -> anyhow::Result<IdentContext> {
        let checkpoint = self.ident_context;
        match node.kind() {
            SyntaxKind::RefMarker => self.ident_context = IdentContext::Ref,
            SyntaxKind::LetBinding => self.ident_context = IdentContext::Ref,
            SyntaxKind::Closure => self.ident_context = IdentContext::Func,
            SyntaxKind::Params => self.ident_context = IdentContext::Params,
            _ => {}
        }

        Ok(checkpoint)
    }

    fn exit_symbol_context(&mut self, checkpoint: IdentContext) -> anyhow::Result<()> {
        self.ident_context = checkpoint;
        Ok(())
    }

    /// Get all symbols for a node recursively.
    fn get_symbols(&mut self, node: LinkedNode) -> anyhow::Result<()> {
        let own_symbol = self.get_ident(&node)?;

        let checkpoint = self.enter_symbol_context(&node)?;

        if let Some(symbol) = own_symbol {
            if let LexicalKind::Namespace(level) = symbol.kind {
                'heading_break: while let Some((w, _)) = self.stack.last() {
                    match w.kind {
                        LexicalKind::Namespace(l) if l < level => break 'heading_break,
                        LexicalKind::Block => break 'heading_break,
                        _ if self.stack.len() <= 1 => break 'heading_break,
                        _ => {}
                    }

                    self.symbreak();
                }
            }
            let is_heading = matches!(symbol.kind, LexicalKind::Namespace(..));

            self.stack.push((symbol, eco_vec![]));
            let stack_height = self.stack.len();

            for child in node.children() {
                self.get_symbols(child)?;
            }

            if is_heading {
                while stack_height < self.stack.len() {
                    self.symbreak();
                }
            } else {
                while stack_height <= self.stack.len() {
                    self.symbreak();
                }
            }
        } else {
            match node.kind() {
                SyntaxKind::LetBinding => 'let_binding: {
                    let name = node.children().find(|n| n.cast::<ast::Pattern>().is_some());

                    if let Some(name) = &name {
                        let p = name.cast::<ast::Pattern>().unwrap();

                        // special case
                        if matches!(p, ast::Pattern::Normal(ast::Expr::Closure(..))) {
                            self.get_symbols_with(name.clone(), IdentContext::Ref)?;
                            break 'let_binding;
                        }
                    }

                    // reverse order for correct symbol affection
                    if self.g == LexicalScopeKind::DefUse {
                        self.get_symbols_in_first_expr(node.children().rev())?;
                        if let Some(name) = name {
                            self.get_symbols_with(name, IdentContext::Var)?;
                        }
                    } else {
                        if let Some(name) = name {
                            self.get_symbols_with(name, IdentContext::Var)?;
                        }
                        self.get_symbols_in_first_expr(node.children().rev())?;
                    }
                }
                SyntaxKind::Closure => {
                    let n = node.children().next();
                    if let Some(n) = n {
                        if n.kind() == SyntaxKind::Ident {
                            self.get_symbols_with(n, IdentContext::Func)?;
                        }
                    }
                    if self.g == LexicalScopeKind::DefUse {
                        let param = node.children().find(|n| n.kind() == SyntaxKind::Params);
                        if let Some(param) = param {
                            self.get_symbols_with(param, IdentContext::Params)?;
                        }
                    }
                    let body = node
                        .children()
                        .rev()
                        .find(|n| n.cast::<ast::Expr>().is_some());
                    if let Some(body) = body {
                        if self.g == LexicalScopeKind::DefUse {
                            let symbol = LexicalInfo {
                                name: String::new(),
                                kind: LexicalKind::Block,
                                range: body.range(),
                            };
                            self.stack.push((symbol, eco_vec![]));
                            let stack_height = self.stack.len();
                            self.get_symbols_with(body, IdentContext::Ref)?;
                            while stack_height <= self.stack.len() {
                                self.symbreak();
                            }
                        } else {
                            self.get_symbols_with(body, IdentContext::Ref)?;
                        }
                    }
                }
                SyntaxKind::FieldAccess => {
                    self.get_symbols_in_first_expr(node.children())?;
                }
                SyntaxKind::Named => {
                    if self.ident_context == IdentContext::Params {
                        let ident = node.children().find(|n| n.kind() == SyntaxKind::Ident);
                        if let Some(ident) = ident {
                            self.get_symbols_with(ident, IdentContext::Var)?;
                        }
                    }

                    self.get_symbols_in_first_expr(node.children().rev())?;
                }
                _ => {
                    for child in node.children() {
                        self.get_symbols(child)?;
                    }
                }
            }
        }

        self.exit_symbol_context(checkpoint)?;

        Ok(())
    }

    fn get_symbols_in_first_expr<'a>(
        &mut self,
        mut nodes: impl Iterator<Item = LinkedNode<'a>>,
    ) -> anyhow::Result<()> {
        let body = nodes.find(|n| n.cast::<ast::Expr>().is_some());
        if let Some(body) = body {
            self.get_symbols_with(body, IdentContext::Ref)?;
        }

        Ok(())
    }

    fn get_symbols_with(&mut self, node: LinkedNode, context: IdentContext) -> anyhow::Result<()> {
        let c = self.ident_context;
        self.ident_context = context;

        let res = self.get_symbols(node);

        self.ident_context = c;
        res
    }

    /// Get symbol for a leaf node of a valid type, or `None` if the node is an
    /// invalid type.
    #[allow(deprecated)]
    fn get_ident(&self, node: &LinkedNode) -> anyhow::Result<Option<LexicalInfo>> {
        let (name, kind) = match node.kind() {
            SyntaxKind::Label if self.g.affect_symbol() => {
                let ast_node = node
                    .cast::<ast::Label>()
                    .ok_or_else(|| anyhow!("cast to ast node failed: {:?}", node))?;
                let name = ast_node.get().to_string();

                (name, LexicalKind::Label)
            }
            SyntaxKind::RefMarker if self.g.affect_ref() => {
                let name = node.text().trim_start_matches('@').to_owned();
                (name, LexicalKind::LabelRef)
            }
            SyntaxKind::Ident if self.g.affect_symbol() => {
                let ast_node = node
                    .cast::<ast::Ident>()
                    .ok_or_else(|| anyhow!("cast to ast node failed: {:?}", node))?;
                let name = ast_node.get().to_string();
                let kind = match self.ident_context {
                    IdentContext::Ref if self.g.affect_ref() => LexicalKind::ValRef,
                    IdentContext::Func => LexicalKind::Function,
                    IdentContext::Var | IdentContext::Params => LexicalKind::Variable,
                    _ => return Ok(None),
                };

                (name, kind)
            }
            SyntaxKind::Equation | SyntaxKind::Raw | SyntaxKind::BlockComment
                if self.g.affect_markup() =>
            {
                (String::new(), LexicalKind::Block)
            }
            SyntaxKind::CodeBlock | SyntaxKind::ContentBlock if self.g.affect_block() => {
                (String::new(), LexicalKind::Block)
            }
            SyntaxKind::Parenthesized
            | SyntaxKind::Destructuring
            | SyntaxKind::Args
            | SyntaxKind::Array
            | SyntaxKind::Dict
                if self.g.affect_expr() =>
            {
                (String::new(), LexicalKind::Block)
            }
            SyntaxKind::Markup => {
                let name = node.get().to_owned().into_text().to_string();
                if name.is_empty() {
                    return Ok(None);
                }
                let Some(parent) = node.parent() else {
                    return Ok(None);
                };
                let kind = match parent.kind() {
                    SyntaxKind::Heading if self.g.affect_heading() => LexicalKind::Namespace(
                        parent.cast::<ast::Heading>().unwrap().depth().get() as i16,
                    ),
                    _ => return Ok(None),
                };

                (name, kind)
            }
            _ => return Ok(None),
        };

        Ok(Some(LexicalInfo {
            name,
            kind,
            range: node.range(),
        }))
    }
}

fn symbreak(sym: LexicalInfo, curr: EcoVec<LexicalHierarchy>) -> LexicalHierarchy {
    LexicalHierarchy {
        info: sym,
        children: if curr.is_empty() {
            None
        } else {
            Some(LazyHash::new(curr))
        },
    }
}
