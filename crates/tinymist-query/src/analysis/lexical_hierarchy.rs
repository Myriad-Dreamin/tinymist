use std::ops::Range;

use anyhow::anyhow;
use log::info;
use lsp_types::SymbolKind;
use typst::syntax::{ast, LinkedNode, Source, SyntaxKind};
use typst_ts_core::typst::prelude::{eco_vec, EcoVec};

#[derive(Debug, Clone, Copy, Hash)]
pub(crate) enum LexicalKind {
    Namespace(i16),
    Variable,
    Function,
    Constant,
    Block,
}

impl TryFrom<LexicalKind> for SymbolKind {
    type Error = ();

    fn try_from(value: LexicalKind) -> Result<Self, Self::Error> {
        match value {
            LexicalKind::Namespace(..) => Ok(SymbolKind::NAMESPACE),
            LexicalKind::Variable => Ok(SymbolKind::VARIABLE),
            LexicalKind::Function => Ok(SymbolKind::FUNCTION),
            LexicalKind::Constant => Ok(SymbolKind::CONSTANT),
            LexicalKind::Block => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, Hash, Default, PartialEq, Eq)]
pub(crate) enum LexicalScopeKind {
    #[default]
    Symbol,
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
    g: LexicalScopeKind,
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
        g: LexicalScopeKind,
        stack: Vec<(LexicalInfo, EcoVec<LexicalHierarchy>)>,
    }

    impl LexicalHierarchyWorker {
        fn symbreak(&mut self) {
            let (symbol, children) = self.stack.pop().unwrap();
            let current = &mut self.stack.last_mut().unwrap().1;

            // symbol.wide_range = children
            //     .iter()
            //     .map(|c| c.info.wide_range.clone())
            //     .fold(symbol.range.clone(), |acc, r| {
            //         acc.start.min(r.start)..acc.end.max(r.end)
            //     });

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
    fn get_ident(node: &LinkedNode, g: LexicalScopeKind) -> anyhow::Result<Option<LexicalInfo>> {
        let (name, kind) = match node.kind() {
            SyntaxKind::Label if LexicalScopeKind::Block != g => {
                let ast_node = node
                    .cast::<ast::Label>()
                    .ok_or_else(|| anyhow!("cast to ast node failed: {:?}", node))?;
                let name = ast_node.get().to_string();

                (name, LexicalKind::Constant)
            }
            SyntaxKind::CodeBlock | SyntaxKind::ContentBlock if LexicalScopeKind::Symbol != g => {
                (String::new(), LexicalKind::Block)
            }
            SyntaxKind::Ident if LexicalScopeKind::Block != g => {
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
