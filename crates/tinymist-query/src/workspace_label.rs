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

        for fid in ctx.source_files().clone() {
            let Ok(source) = ctx.source_by_id(fid) else {
                continue;
            };
            let Ok(path) = ctx.path_for_id(fid) else {
                continue;
            };
            let uri = path_to_url(&path).unwrap();
            let res = get_lexical_hierarchy(&source, LexicalScopeKind::Symbol).map(|hierarchy| {
                filter_document_labels(&hierarchy, &source, &uri, ctx.position_encoding())
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
    hierarchy: &[LexicalHierarchy],
    source: &Source,
    uri: &Url,
    position_encoding: PositionEncoding,
) -> Vec<SymbolInformation> {
    hierarchy
        .iter()
        .flat_map(|hierarchy| {
            [hierarchy]
                .into_iter()
                .chain(hierarchy.children.as_deref().into_iter().flatten())
        })
        .flat_map(|hierarchy| {
            if !matches!(hierarchy.info.kind, LexicalKind::Var(LexicalVarKind::Label)) {
                return None;
            }

            let rng = to_lsp_range(hierarchy.info.range.clone(), source, position_encoding);

            Some(SymbolInformation {
                name: hierarchy.info.name.to_string(),
                kind: hierarchy.info.kind.clone().try_into().unwrap(),
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
