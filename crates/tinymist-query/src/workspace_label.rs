use crate::{
    prelude::*,
    syntax::{
        get_lexical_hierarchy, LexicalHierarchy, LexicalKind, LexicalScopeKind, LexicalVarKind,
    },
    SemanticRequest,
};

/// The `workspace/label` request resembles [`workspace/symbol`] request but is
/// extended for typst cases.
///
/// [`workspace/symbol`]: https://microsoft.github.io/language-server-protocol/specification#workspace_symbol
#[derive(Debug, Clone)]
pub struct WorkspaceLabelRequest {}

impl SemanticRequest for WorkspaceLabelRequest {
    type Response = Vec<SymbolInformation>;

    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response> {
        // todo: let typst.ts expose source

        let mut symbols = vec![];

        for id in ctx.source_files().clone() {
            let Ok(source) = ctx.source_by_id(id) else {
                continue;
            };
            let Ok(path) = ctx.path_for_id(id) else {
                continue;
            };
            let uri = path_to_url(&path).unwrap();
            let res =
                get_lexical_hierarchy(source.clone(), LexicalScopeKind::Symbol).map(|symbols| {
                    filter_document_labels(&symbols, &source, &uri, ctx.position_encoding())
                });

            if let Some(mut res) = res {
                symbols.append(&mut res)
            }
        }

        Some(symbols)
    }
}

#[allow(deprecated)]
fn filter_document_labels(
    symbols: &[LexicalHierarchy],
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
        .flat_map(|e| {
            if !matches!(e.info.kind, LexicalKind::Var(LexicalVarKind::Label)) {
                return None;
            }

            let rng = typst_to_lsp::range(e.info.range.clone(), source, position_encoding);

            Some(SymbolInformation {
                name: e.info.name.to_string(),
                kind: e.info.kind.clone().try_into().unwrap(),
                tags: None,
                deprecated: None,
                location: LspLocation {
                    uri: uri.clone(),
                    range: rng,
                },
                container_name: None,
            })
        })
        .collect()
}
