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
        let mut symbols = vec![];

        for id in ctx.depended_files() {
            let Ok(source) = ctx.source_by_id(id) else {
                continue;
            };
            let uri = ctx.uri_for_id(id).unwrap();
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
        .filter(|hierarchy| hierarchy.info.kind.is_valid_lsp_symbol())
        .flat_map(|hierarchy| {
            if query_string.is_some_and(|s| !hierarchy.info.name.contains(s)) {
                return None;
            }

            let rng = to_lsp_range(hierarchy.info.range.clone(), source, position_encoding);

            Some(SymbolInformation {
                name: hierarchy.info.name.to_string(),
                kind: hierarchy.info.kind.clone().into(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::find_module_level_docs;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("symbols", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let docs = find_module_level_docs(&source).unwrap_or_default();
            let mut properties = get_test_properties(&docs);
            // need to compile the doc to get the dependencies
            properties.insert("compile", "true");
            let _doc = compile_doc_for_test(ctx, &properties);

            let request = SymbolRequest {
                pattern: properties.get("pattern").copied().map(str::to_owned),
            };

            let result = request.request(ctx);
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
