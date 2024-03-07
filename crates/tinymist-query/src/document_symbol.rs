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
        world: &TypstSystemWorld,
        position_encoding: PositionEncoding,
    ) -> Option<DocumentSymbolResponse> {
        let source = get_suitable_source_in_workspace(world, &self.path).ok()?;

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
            let rng =
                typst_to_lsp::range(e.info.range.clone(), source, position_encoding).raw_range;

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
