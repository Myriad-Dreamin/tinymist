use crate::{
    analysis::{get_lexical_hierarchy, LexicalHierarchy, LexicalScopeKind},
    prelude::*,
};

#[derive(Debug, Clone)]
pub struct DocumentSymbolRequest {
    pub path: PathBuf,
}

impl DocumentSymbolRequest {
    pub fn request(
        self,
        source: Source,
        position_encoding: PositionEncoding,
    ) -> Option<DocumentSymbolResponse> {
        let symbols = get_lexical_hierarchy(source.clone(), LexicalScopeKind::Symbol)?;

        let symbols = filter_document_symbols(&symbols, &source, position_encoding);
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
                name: e.info.name.clone(),
                detail: None,
                kind: e.info.kind.try_into().unwrap(),
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
        snapshot_testing("document_symbols", &|world, path| {
            let request = DocumentSymbolRequest { path: path.clone() };

            let source = get_suitable_source_in_workspace(world, &path).unwrap();

            let result = request.request(source, PositionEncoding::Utf16);
            assert_snapshot!(JsonRepr::new_redacted(result.unwrap(), &REDACT_LOC));
        });
    }
}
