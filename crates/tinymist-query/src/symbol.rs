use crate::{
    prelude::*,
    syntax::{get_lexical_hierarchy, LexicalHierarchy, LexicalScopeKind},
    SemanticRequest,
};

/// The [`workspace/symbol`] request is sent from the client to the server to
/// list project-wide symbols matching the given query string.
///
/// [`workspace/symbol`]: https://microsoft.github.io/language-server-protocol/specification#workspace_symbol
///
/// # Compatibility
///
/// Since 3.17.0, servers can also provider a handler for
/// [`workspaceSymbol/resolve`] requests. This allows servers to return
/// workspace symbols without a range for a `workspace/symbol` request. Clients
/// then need to resolve the range when necessary using the `workspaceSymbol/
/// resolve` request.
///
/// // [`workspaceSymbol/resolve`]: Self::symbol_resolve
///
/// Servers can only use this new model if clients advertise support for it via
/// the `workspace.symbol.resolve_support` capability.
#[derive(Debug, Clone)]
pub struct SymbolRequest {
    /// The query string to filter symbols by. It is usually the exact content
    /// of the user's input box in the UI.
    pub pattern: Option<String>,
}

impl SemanticRequest for SymbolRequest {
    type Response = Vec<SymbolInformation>;

    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response> {
        // todo: let typst.ts expose source

        let mut symbols = vec![];

        // todo! need compilation for iter_dependencies

        for path in ctx.dependencies() {
            let Ok(source) = ctx.source_by_path(&path) else {
                continue;
            };
            let uri = path_to_url(&path).unwrap();
            let res = get_lexical_hierarchy(&source, LexicalScopeKind::Symbol).map(|symbols| {
                filter_document_symbols(
                    &symbols,
                    self.pattern.as_deref(),
                    &source,
                    &uri,
                    ctx.position_encoding(),
                )
            });

            if let Some(mut res) = res {
                symbols.append(&mut res)
            }
        }

        Some(symbols)
    }
}

#[allow(deprecated)]
fn filter_document_symbols(
    hierarchy: &[LexicalHierarchy],
    query_string: Option<&str>,
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
            if query_string.is_some_and(|s| !hierarchy.info.name.contains(s)) {
                return None;
            }

            let rng = typst_to_lsp::range(hierarchy.info.range.clone(), source, position_encoding);

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
