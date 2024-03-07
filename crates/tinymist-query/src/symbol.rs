use typst_ts_compiler::NotifyApi;

use crate::document_symbol::get_document_symbols;
use crate::prelude::*;

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
            let res = get_document_symbols(source, uri, position_encoding).and_then(|symbols| {
                self.pattern
                    .as_ref()
                    .map(|pattern| filter_document_symbols(symbols, pattern))
            });

            if let Some(mut res) = res {
                symbols.append(&mut res)
            }
        });

        Some(symbols)
    }
}

fn filter_document_symbols(
    symbols: Vec<SymbolInformation>,
    query_string: &str,
) -> Vec<SymbolInformation> {
    symbols
        .into_iter()
        .filter(|e| e.name.contains(query_string))
        .collect()
}
