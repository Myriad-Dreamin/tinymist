use typst_ts_compiler::NotifyApi;

use crate::{
    analysis::{get_lexical_hierarchy, LexicalHierarchy, LexicalScopeKind},
    prelude::*,
};

#[derive(Debug, Clone)]
pub struct SymbolRequest {
    pub pattern: Option<String>,
}

impl SymbolRequest {
    pub fn request(
        self,
        world: &TypstSystemWorld,
        position_encoding: PositionEncoding,
    ) -> Option<Vec<SymbolInformation>> {
        // todo: expose source

        let mut symbols = vec![];

        world.iter_dependencies(&mut |path, _| {
            let Ok(source) = get_suitable_source_in_workspace(world, path) else {
                return;
            };
            let uri = Url::from_file_path(path).unwrap();
            let res = get_lexical_hierarchy(source.clone(), LexicalScopeKind::Symbol).and_then(
                |symbols| {
                    self.pattern.as_ref().map(|pattern| {
                        filter_document_symbols(&symbols, pattern, &source, &uri, position_encoding)
                    })
                },
            );

            if let Some(mut res) = res {
                symbols.append(&mut res)
            }
        });

        Some(symbols)
    }
}

#[allow(deprecated)]
fn filter_document_symbols(
    symbols: &[LexicalHierarchy],
    query_string: &str,
    source: &Source,
    uri: &Url,
    position_encoding: PositionEncoding,
) -> Vec<SymbolInformation> {
    symbols
        .iter()
        .flat_map(|e| {
            [e].into_iter()
                .chain(e.children.as_deref().into_iter().flatten())
        })
        .filter(|e| e.info.name.contains(query_string))
        .map(|e| {
            let rng = typst_to_lsp::range(e.info.range.clone(), source, position_encoding);

            SymbolInformation {
                name: e.info.name.clone(),
                kind: e.info.kind.clone().try_into().unwrap(),
                tags: None,
                deprecated: None,
                location: LspLocation {
                    uri: uri.clone(),
                    range: rng,
                },
                container_name: None,
            }
        })
        .collect()
}
