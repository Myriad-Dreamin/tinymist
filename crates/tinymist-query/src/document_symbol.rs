use std::ops::Range;

use typst_ts_core::typst::prelude::{eco_vec, EcoVec};

use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct DocumentSymbolRequest {
    pub path: PathBuf,
}

impl DocumentSymbolRequest {
    pub fn request(
        self,
        world: &TypstSystemWorld,
        position_encoding: PositionEncoding,
    ) -> Option<DocumentSymbolResponse> {
        let source = get_suitable_source_in_workspace(world, &self.path).ok()?;

        let symbols = get_lexical_hierarchy(source.clone(), LexicalScopeGranularity::None);

        let symbols =
            symbols.map(|symbols| filter_document_symbols(&symbols, &source, position_encoding));
        symbols.map(DocumentSymbolResponse::Nested)
    }
}

#[allow(deprecated)]
fn filter_document_symbols(
    symbols: &[LexicalHierarchy],
    source: &Source,
    position_encoding: PositionEncoding,
) -> Vec<DocumentSymbol> {
    symbols
        .iter()
        .map(|e| {
            let rng =
                typst_to_lsp::range(e.info.range.clone(), source, position_encoding).raw_range;

            DocumentSymbol {
                name: e.info.name.clone(),
                detail: None,
                kind: match e.info.kind {
                    LexicalKind::Namespace(..) => SymbolKind::NAMESPACE,
                    LexicalKind::Variable => SymbolKind::VARIABLE,
                    LexicalKind::Function => SymbolKind::FUNCTION,
                    LexicalKind::Constant => SymbolKind::CONSTANT,
                    LexicalKind::Block => unreachable!(),
                },
                tags: None,
                deprecated: None,
                range: rng,
                selection_range: rng,
                //             .raw_range,
                children: e
                    .children
                    .as_ref()
                    .map(|ch| filter_document_symbols(ch, source, position_encoding)),
            }
        })
        .collect()
}

#[derive(Debug, Clone, Hash)]
pub(crate) enum LexicalKind {
    Namespace(i16),
    Variable,
    Function,
    Constant,
    Block,
}

#[derive(Debug, Clone, Copy, Hash, Default, PartialEq, Eq)]
pub(crate) enum LexicalScopeGranularity {
    #[default]
    None,
    Block,
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
    pub children: Option<comemo::Prehashed<EcoVec<LexicalHierarchy>>>,
}

pub(crate) fn get_lexical_hierarchy(
    source: Source,
    g: LexicalScopeGranularity,
) -> Option<EcoVec<LexicalHierarchy>> {
    fn symbreak(sym: LexicalInfo, curr: EcoVec<LexicalHierarchy>) -> LexicalHierarchy {
        LexicalHierarchy {
            info: sym,
            children: if curr.is_empty() {
                None
            } else {
                Some(comemo::Prehashed::new(curr))
            },
        }
    }

    #[derive(Default)]
    struct LexicalHierarchyWorker {
        g: LexicalScopeGranularity,
        stack: Vec<(LexicalInfo, EcoVec<LexicalHierarchy>)>,
    }

    impl LexicalHierarchyWorker {
        fn symbreak(&mut self) {
            let (symbol, children) = self.stack.pop().unwrap();
            let current = &mut self.stack.last_mut().unwrap().1;
            current.push(symbreak(symbol, children));
        }

        /// Get all symbols for a node recursively.
        fn get_symbols(&mut self, node: LinkedNode) -> anyhow::Result<()> {
            let own_symbol = get_ident(&node, self.g)?;

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
                for child in node.children() {
                    self.get_symbols(child)?;
                }
            }

            Ok(())
        }
    }

    /// Get symbol for a leaf node of a valid type, or `None` if the node is an
    /// invalid type.
    #[allow(deprecated)]
    fn get_ident(
        node: &LinkedNode,
        g: LexicalScopeGranularity,
    ) -> anyhow::Result<Option<LexicalInfo>> {
        let (name, kind) = match node.kind() {
            SyntaxKind::Label => {
                let ast_node = node
                    .cast::<ast::Label>()
                    .ok_or_else(|| anyhow!("cast to ast node failed: {:?}", node))?;
                let name = ast_node.get().to_string();

                (name, LexicalKind::Constant)
            }
            SyntaxKind::CodeBlock | SyntaxKind::ContentBlock
                if LexicalScopeGranularity::None != g =>
            {
                (String::new(), LexicalKind::Block)
            }
            SyntaxKind::Ident => {
                let ast_node = node
                    .cast::<ast::Ident>()
                    .ok_or_else(|| anyhow!("cast to ast node failed: {:?}", node))?;
                let name = ast_node.get().to_string();
                let Some(parent) = node.parent() else {
                    return Ok(None);
                };
                let kind = match parent.kind() {
                    // for variable definitions, the Let binding holds an Ident
                    SyntaxKind::LetBinding => LexicalKind::Variable,
                    // for function definitions, the Let binding holds a Closure which holds the
                    // Ident
                    SyntaxKind::Closure => {
                        let Some(grand_parent) = parent.parent() else {
                            return Ok(None);
                        };
                        match grand_parent.kind() {
                            SyntaxKind::LetBinding => LexicalKind::Function,
                            _ => return Ok(None),
                        }
                    }
                    _ => return Ok(None),
                };

                (name, kind)
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
                    SyntaxKind::Heading => LexicalKind::Namespace(
                        parent.cast::<ast::Heading>().unwrap().level().get() as i16,
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
    res.map(|_| worker.stack.pop().unwrap().1)
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use super::*;
    use crate::tests::*;

    #[test]
    fn test_get_document_symbols() {
        run_with_source(
            r#"
= Heading 1
#let a = 1;
== Heading 2
#let b = 1;
= Heading 3
#let c = 1;
#let d = {
  #let e = 1;
  0
}
"#,
            |world, path| {
                let request = DocumentSymbolRequest { path };
                let result = request.request(world, PositionEncoding::Utf16);
                assert_snapshot!(JsonRepr::new_redacted(result.unwrap(), &REDACT_LOC), @r###"
                [
                 {
                  "children": [
                   {
                    "kind": 13,
                    "name": "a"
                   },
                   {
                    "children": [
                     {
                      "kind": 13,
                      "name": "b"
                     }
                    ],
                    "kind": 3,
                    "name": "Heading 2"
                   }
                  ],
                  "kind": 3,
                  "name": "Heading 1"
                 },
                 {
                  "children": [
                   {
                    "kind": 13,
                    "name": "c"
                   },
                   {
                    "kind": 13,
                    "name": "d"
                   },
                   {
                    "kind": 13,
                    "name": "e"
                   }
                  ],
                  "kind": 3,
                  "name": "Heading 3"
                 }
                ]
                "###);
            },
        );
    }
}
