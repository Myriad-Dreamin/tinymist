use lsp_types::SymbolKind;

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
        let hierarchy = get_lexical_hierarchy(source, LexicalScopeKind::Symbol)?;
        let symbols = symbols_in_hierarchy(&hierarchy, source, position_encoding);
        Some(DocumentSymbolResponse::Nested(symbols))
    }
}

#[allow(deprecated)]
fn symbols_in_hierarchy(
    hierarchy: &[LexicalHierarchy],
    source: &Source,
    position_encoding: PositionEncoding,
) -> Vec<DocumentSymbol> {
    hierarchy
        .iter()
        .filter(|hierarchy| TryInto::<SymbolKind>::try_into(hierarchy.info.kind.clone()).is_ok())
        .map(|hierarchy| {
            let range =
                typst_to_lsp::range(hierarchy.info.range.clone(), source, position_encoding);

            DocumentSymbol {
                name: hierarchy.info.name.to_string(),
                detail: None,
                kind: hierarchy.info.kind.clone().try_into().unwrap(),
                tags: None,
                deprecated: None,
                range,
                selection_range: range,
                children: hierarchy
                    .children
                    .as_ref()
                    .map(|ch| symbols_in_hierarchy(ch, source, position_encoding)),
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
