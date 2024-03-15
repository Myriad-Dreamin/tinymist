use std::{
    ops::{Deref, Range},
    path::Path,
};

use anyhow::{anyhow, Context};
use log::info;
use lsp_types::SymbolKind;
use serde::{Deserialize, Serialize};
use typst::{
    syntax::{
        ast::{self, AstNode},
        LinkedNode, Source, SyntaxKind,
    },
    util::LazyHash,
};
use typst_ts_core::typst::prelude::{eco_vec, EcoVec};

use super::IdentRef;

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
            kind: LexicalKind::Heading(-1),
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

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModSrc {
    /// `import cetz.draw ...`
    ///  ^^^^^^^^^^^^^^^^^^^^
    Expr(Box<IdentRef>),
    /// `import "" ...`
    ///  ^^^^^^^^^^^^^
    Path(Box<str>),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum LexicalModKind {
    /// See [`ModSrc`]
    Module(ModSrc),
    /// `import "foo" as bar;`
    ///                  ^^^
    ModuleAlias,
    /// `import "foo.typ"`
    ///          ^^^
    PathVar,
    /// `import "foo": bar`
    ///                ^^^
    Ident,
    /// `import "foo": bar as baz`
    ///                ^^^^^^^^^^
    Alias { target: Box<IdentRef> },
    /// `import "foo": *`
    ///                ^
    Star,
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
    Mod(LexicalModKind),
    Block,
}

impl LexicalKind {
    const fn label() -> LexicalKind {
        LexicalKind::Var(LexicalVarKind::Label)
    }

    const fn label_ref() -> LexicalKind {
        LexicalKind::Var(LexicalVarKind::LabelRef)
    }

    const fn val_ref() -> LexicalKind {
        LexicalKind::Var(LexicalVarKind::ValRef)
    }

    const fn function() -> LexicalKind {
        LexicalKind::Var(LexicalVarKind::Function)
    }

    const fn variable() -> LexicalKind {
        LexicalKind::Var(LexicalVarKind::Variable)
    }

    const fn module_as() -> LexicalKind {
        LexicalKind::Mod(LexicalModKind::ModuleAlias)
    }

    const fn module_path() -> LexicalKind {
        LexicalKind::Mod(LexicalModKind::PathVar)
    }

    const fn module_import() -> LexicalKind {
        LexicalKind::Mod(LexicalModKind::Ident)
    }

    const fn module_star() -> LexicalKind {
        LexicalKind::Mod(LexicalModKind::Star)
    }

    fn module_expr(path: Box<IdentRef>) -> LexicalKind {
        LexicalKind::Mod(LexicalModKind::Module(ModSrc::Expr(path)))
    }

    fn module(path: Box<str>) -> LexicalKind {
        LexicalKind::Mod(LexicalModKind::Module(ModSrc::Path(path)))
    }

    fn module_import_alias(alias: IdentRef) -> LexicalKind {
        LexicalKind::Mod(LexicalModKind::Alias {
            target: Box::new(alias),
        })
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
            LexicalKind::Var(..) | LexicalKind::Mod(..) | LexicalKind::Block => Err(()),
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

    fn affect_import(&self) -> bool {
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
    ModImport,
    Params,
}

#[derive(Default)]
struct LexicalHierarchyWorker {
    g: LexicalScopeKind,
    stack: Vec<(LexicalInfo, EcoVec<LexicalHierarchy>)>,
    ident_context: IdentContext,
}

impl LexicalHierarchyWorker {
    fn push_leaf(&mut self, symbol: LexicalInfo) {
        let current = &mut self.stack.last_mut().unwrap().1;
        current.push(LexicalHierarchy {
            info: symbol,
            children: None,
        });
    }

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
            if let LexicalKind::Heading(level) = symbol.kind {
                'heading_break: while let Some((w, _)) = self.stack.last() {
                    match w.kind {
                        LexicalKind::Heading(l) if l < level => break 'heading_break,
                        LexicalKind::Block => break 'heading_break,
                        _ if self.stack.len() <= 1 => break 'heading_break,
                        _ => {}
                    }

                    self.symbreak();
                }
            }
            let is_heading = matches!(symbol.kind, LexicalKind::Heading(..));

            self.stack.push((symbol, eco_vec![]));
            let stack_height = self.stack.len();

            if node.kind() == SyntaxKind::ModuleImport {
                self.get_symbols_in_import(node)?;
            } else {
                for child in node.children() {
                    self.get_symbols(child)?;
                }
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

                            if self.g == LexicalScopeKind::DefUse {
                                let param =
                                    node.children().find(|n| n.kind() == SyntaxKind::Params);
                                if let Some(param) = param {
                                    self.get_symbols_with(param, IdentContext::Params)?;
                                }
                            }

                            self.get_symbols_with(body, IdentContext::Ref)?;
                            while stack_height <= self.stack.len() {
                                self.symbreak();
                            }
                        } else {
                            self.get_symbols_with(body, IdentContext::Ref)?;
                        }
                    }
                }
                SyntaxKind::RenamedImportItem if self.g.affect_import() => {
                    let src = node
                        .cast::<ast::RenamedImportItem>()
                        .ok_or_else(|| anyhow!("cast to renamed import item failed: {:?}", node))?;

                    let origin_name = src.new_name();
                    let origin_name_node = node.find(origin_name.span()).context("no pos")?;

                    let target_name = src.original_name();
                    let target_name_node = node.find(target_name.span()).context("no pos")?;

                    self.push_leaf(LexicalInfo {
                        name: origin_name.get().to_string(),
                        kind: LexicalKind::module_import_alias(IdentRef {
                            name: target_name.get().to_string(),
                            range: target_name_node.range(),
                        }),
                        range: origin_name_node.range(),
                    });
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

                (name, LexicalKind::label())
            }
            SyntaxKind::RefMarker if self.g.affect_ref() => {
                let name = node.text().trim_start_matches('@').to_owned();
                (name, LexicalKind::label_ref())
            }
            SyntaxKind::Ident if self.g.affect_symbol() => {
                let ast_node = node
                    .cast::<ast::Ident>()
                    .ok_or_else(|| anyhow!("cast to ast node failed: {:?}", node))?;
                let name = ast_node.get().to_string();
                let kind = match self.ident_context {
                    IdentContext::Ref if self.g.affect_ref() => LexicalKind::val_ref(),
                    IdentContext::Func => LexicalKind::function(),
                    IdentContext::Var | IdentContext::Params => LexicalKind::variable(),
                    IdentContext::ModImport => LexicalKind::module_import(),
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
            SyntaxKind::ModuleImport if self.g.affect_import() => {
                let src = node
                    .cast::<ast::ModuleImport>()
                    .ok_or_else(|| anyhow!("cast to module import failed: {:?}", node))?
                    .source();

                match src {
                    ast::Expr::Str(e) => {
                        let e = e.get();
                        (String::new(), LexicalKind::module(e.as_ref().into()))
                    }
                    src => {
                        let e = node
                            .find(src.span())
                            .ok_or_else(|| anyhow!("find expression failed: {:?}", src))?;
                        let e = IdentRef {
                            name: String::new(),
                            range: e.range(),
                        };

                        (String::new(), LexicalKind::module_expr(e.into()))
                    }
                }
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
                    SyntaxKind::Heading if self.g.affect_heading() => LexicalKind::Heading(
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

    fn get_symbols_in_import(&mut self, node: LinkedNode) -> anyhow::Result<()> {
        // todo: other kind
        if self.g != LexicalScopeKind::DefUse {
            return Ok(());
        }

        let import_node = node.cast::<ast::ModuleImport>().context("not a import")?;
        let v = import_node.source();
        let v_linked = node.find(v.span()).context("no source pos")?;
        match v {
            ast::Expr::Str(..) => {}
            _ => {
                self.get_symbols_with(v_linked.clone(), IdentContext::Ref)?;
            }
        }

        let imports = import_node.imports();
        if let Some(name) = import_node.new_name() {
            // push `import "foo" as bar;`
            //                       ^^^
            let import_node = node.find(name.span()).context("no pos")?;
            self.push_leaf(LexicalInfo {
                name: name.get().to_string(),
                kind: LexicalKind::module_as(),
                range: import_node.range(),
            });

            // note: we can have both:
            // `import "foo" as bar;`
            // `import "foo": bar as baz;`
        } else if imports.is_none() {
            let v = import_node.source();
            match v {
                ast::Expr::Str(e) => {
                    let e = e.get();
                    let e = Path::new(e.as_ref())
                        .file_name()
                        .context("no file name")?
                        .to_string_lossy();
                    let e = e.as_ref();
                    let e = e.strip_suffix(".typ").context("no suffix")?;
                    // return (e == name).then_some(ImportRef::Path(v));
                    self.push_leaf(LexicalInfo {
                        name: e.to_string(),
                        kind: LexicalKind::module_path(),
                        range: v_linked.range(),
                    });
                }
                _ => {
                    // todo: import expr?
                }
            }
            return Ok(());
        };

        let Some(imports) = imports else {
            return Ok(());
        };

        match imports {
            ast::Imports::Wildcard => {
                let wildcard = node
                    .children()
                    .find(|node| node.kind() == SyntaxKind::Star)
                    .context("no star")?;
                let v = node.find(wildcard.span()).context("no pos")?;
                self.push_leaf(LexicalInfo {
                    name: "*".to_string(),
                    kind: LexicalKind::module_star(),
                    range: v.range(),
                });
            }
            ast::Imports::Items(items) => {
                let n = node.find(items.span()).context("no pos")?;
                self.get_symbols_with(n, IdentContext::ModImport)?;
            }
        }

        Ok(())
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
