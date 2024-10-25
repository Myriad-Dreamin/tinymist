use crate::prelude::*;

/// The [`textDocument/definition`] request asks the server for the definition
/// location of a symbol at a given text document position.
///
/// [`textDocument/definition`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_definition
///
/// # Compatibility
///
/// The [`GotoDefinitionResponse::Link`](lsp_types::GotoDefinitionResponse::Link) return value
/// was introduced in specification version 3.14.0 and requires client-side
/// support in order to be used. It can be returned if the client set the
/// following field to `true` in the `initialize` method:
///
/// ```text
/// InitializeParams::capabilities::text_document::definition::link_support
/// ```
#[derive(Debug, Clone)]
pub struct GotoDefinitionRequest {
    /// The path of the document to request for.
    pub path: PathBuf,
    /// The source code position to request for.
    pub position: LspPosition,
}

impl StatefulRequest for GotoDefinitionRequest {
    type Response = GotoDefinitionResponse;

    fn request(
        self,
        ctx: &mut AnalysisContext,
        doc: Option<VersionedDocument>,
    ) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let deref_target = ctx.deref_syntax_at(&source, self.position, 1)?;
        let origin_selection_range = ctx.to_lsp_range(deref_target.node().range(), &source);

        let def = ctx.definition(source.clone(), doc.as_ref(), deref_target)?;

        let (fid, def_range) = def.def_at(ctx.shared())?;

        let uri = ctx.uri_for_id(fid).ok()?;

        let range = ctx.to_lsp_range_(def_range, fid)?;

        let res = Some(GotoDefinitionResponse::Link(vec![LocationLink {
            origin_selection_range: Some(origin_selection_range),
            target_uri: uri,
            target_range: range,
            target_selection_range: range,
        }]));

        log::debug!("goto_definition: {fid:?} {res:?}");
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::find_module_level_docs;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("goto_definition", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let docs = find_module_level_docs(&source).unwrap_or_default();
            let properties = get_test_properties(&docs);
            let doc = compile_doc_for_test(ctx, &properties);

            let request = GotoDefinitionRequest {
                path: path.clone(),
                position: find_test_position(&source),
            };

            let result = request.request(ctx, doc.clone());
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
