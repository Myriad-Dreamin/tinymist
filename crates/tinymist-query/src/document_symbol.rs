use crate::{
    prelude::*,
    syntax::{get_lexical_hierarchy, LexicalHierarchy, LexicalScopeKind},
    SyntaxRequest,
};

/// The [`textDocument/documentSymbol`] request is sent from the client to the
/// server to retrieve all symbols found in a given text document.
///
/// [`textDocument/documentSymbol`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_documentSymbol
///
/// The returned result is either:
///
/// * [`DocumentSymbolResponse::Flat`] which is a flat list of all symbols found
///   in a given text document. Then neither the symbol’s location range nor the
///   symbol’s container name should be used to infer a hierarchy.
/// * [`DocumentSymbolResponse::Nested`] which is a hierarchy of symbols found
///   in a given text document.
#[derive(Debug, Clone)]
pub struct DocumentSymbolRequest {
    /// The path of the document to retrieve symbols from.
    pub path: PathBuf,
}

impl SyntaxRequest for DocumentSymbolRequest {
    type Response = DocumentSymbolResponse;

    fn request(
        self,
        source: &Source,
        position_encoding: PositionEncoding,
    ) -> Option<Self::Response> {
        let symbols = get_lexical_hierarchy(source.clone(), LexicalScopeKind::Symbol)?;

        let symbols = filter_document_symbols(&symbols, source, position_encoding);
        Some(DocumentSymbolResponse::Nested(symbols))
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
            let rng = typst_to_lsp::range(e.info.range.clone(), source, position_encoding);

            DocumentSymbol {
                name: e.info.name.to_string(),
                detail: None,
                kind: e.info.kind.clone().try_into().unwrap(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("document_symbols", &|ctx, path| {
            let request = DocumentSymbolRequest { path: path.clone() };

            let source = ctx.source_by_path(&path).unwrap();

            let result = request.request(&source, PositionEncoding::Utf16);
            assert_snapshot!(JsonRepr::new_redacted(result.unwrap(), &REDACT_LOC));
        });
    }
}
