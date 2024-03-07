use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct DocumentSymbolRequest {
    pub path: PathBuf,
}

pub fn document_symbol(
    world: &TypstSystemWorld,
    req: DocumentSymbolRequest,
    position_encoding: PositionEncoding,
) -> Option<DocumentSymbolResponse> {
    let source = get_suitable_source_in_workspace(world, &req.path).ok()?;

    let uri = Url::from_file_path(req.path).unwrap();
    let symbols = get_document_symbols(source, uri, position_encoding);

    symbols.map(DocumentSymbolResponse::Flat)
}

#[comemo::memoize]
pub(crate) fn get_document_symbols(
    source: Source,
    uri: Url,
    position_encoding: PositionEncoding,
) -> Option<Vec<SymbolInformation>> {
    struct DocumentSymbolWorker {
        symbols: Vec<SymbolInformation>,
    }

    impl DocumentSymbolWorker {
        /// Get all symbols for a node recursively.
        pub fn get_symbols<'a>(
            &mut self,
            node: LinkedNode<'a>,
            source: &'a Source,
            uri: &'a Url,
            position_encoding: PositionEncoding,
        ) -> anyhow::Result<()> {
            let own_symbol = get_ident(&node, source, uri, position_encoding)?;

            for child in node.children() {
                self.get_symbols(child, source, uri, position_encoding)?;
            }

            if let Some(symbol) = own_symbol {
                self.symbols.push(symbol);
            }

            Ok(())
        }
    }

    /// Get symbol for a leaf node of a valid type, or `None` if the node is an
    /// invalid type.
    #[allow(deprecated)]
    fn get_ident(
        node: &LinkedNode,
        source: &Source,
        uri: &Url,
        position_encoding: PositionEncoding,
    ) -> anyhow::Result<Option<SymbolInformation>> {
        match node.kind() {
            SyntaxKind::Label => {
                let ast_node = node
                    .cast::<ast::Label>()
                    .ok_or_else(|| anyhow!("cast to ast node failed: {:?}", node))?;
                let name = ast_node.get().to_string();
                let symbol = SymbolInformation {
                    name,
                    kind: SymbolKind::CONSTANT,
                    tags: None,
                    deprecated: None, // do not use, deprecated, use `tags` instead
                    location: LspLocation {
                        uri: uri.clone(),
                        range: typst_to_lsp::range(node.range(), source, position_encoding)
                            .raw_range,
                    },
                    container_name: None,
                };
                Ok(Some(symbol))
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
                    SyntaxKind::LetBinding => SymbolKind::VARIABLE,
                    // for function definitions, the Let binding holds a Closure which holds the
                    // Ident
                    SyntaxKind::Closure => {
                        let Some(grand_parent) = parent.parent() else {
                            return Ok(None);
                        };
                        match grand_parent.kind() {
                            SyntaxKind::LetBinding => SymbolKind::FUNCTION,
                            _ => return Ok(None),
                        }
                    }
                    _ => return Ok(None),
                };
                let symbol = SymbolInformation {
                    name,
                    kind,
                    tags: None,
                    deprecated: None, // do not use, deprecated, use `tags` instead
                    location: LspLocation {
                        uri: uri.clone(),
                        range: typst_to_lsp::range(node.range(), source, position_encoding)
                            .raw_range,
                    },
                    container_name: None,
                };
                Ok(Some(symbol))
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
                    SyntaxKind::Heading => SymbolKind::NAMESPACE,
                    _ => return Ok(None),
                };
                let symbol = SymbolInformation {
                    name,
                    kind,
                    tags: None,
                    deprecated: None, // do not use, deprecated, use `tags` instead
                    location: LspLocation {
                        uri: uri.clone(),
                        range: typst_to_lsp::range(node.range(), source, position_encoding)
                            .raw_range,
                    },
                    container_name: None,
                };
                Ok(Some(symbol))
            }
            _ => Ok(None),
        }
    }

    let root = LinkedNode::new(source.root());

    let mut worker = DocumentSymbolWorker { symbols: vec![] };

    let res = worker
        .get_symbols(root, &source, &uri, position_encoding)
        .ok();

    res.map(|_| worker.symbols)
}
