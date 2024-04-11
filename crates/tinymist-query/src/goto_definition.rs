use log::debug;

use crate::{analysis::find_definition, prelude::*, syntax::get_deref_target, SemanticRequest};

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

impl SemanticRequest for GotoDefinitionRequest {
    type Response = GotoDefinitionResponse;

    fn request(self, ctx: &mut AnalysisContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let offset = ctx.to_typst_pos(self.position, &source)?;
        let cursor = offset + 1;

        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        debug!("ast_node: {ast_node:?}", ast_node = ast_node);

        let deref_target = get_deref_target(ast_node, cursor)?;
        let use_site = deref_target.node().clone();
        let origin_selection_range = ctx.to_lsp_range(use_site.range(), &source);

        let def = find_definition(ctx, source.clone(), deref_target)?;

        let (fid, def_range) = def.def_at?;

        let span_path = ctx.path_for_id(fid).ok()?;
        let uri = path_to_url(&span_path).ok()?;

        let span_source = ctx.source_by_id(fid).ok()?;
        let range = ctx.to_lsp_range(def_range, &span_source);

        let res = Some(GotoDefinitionResponse::Link(vec![LocationLink {
            origin_selection_range: Some(origin_selection_range),
            target_uri: uri,
            target_range: range,
            target_selection_range: range,
        }]));

        debug!("goto_definition: {fid:?} {res:?}");
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("goto_definition", &|world, path| {
            let source = world.source_by_path(&path).unwrap();

            let request = GotoDefinitionRequest {
                path: path.clone(),
                position: find_test_position(&source),
            };

            let result = request.request(world);
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
