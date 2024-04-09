use std::ops::Range;

use log::debug;
use lsp_types::LocationLink;

use crate::{
    prelude::*,
    syntax::{get_deref_target, DerefTarget},
    SemanticRequest,
};

/// The [`textDocument/declaration`] request asks the server for the declaration
/// location of a symbol at a given text document position.
///
/// [`textDocument/declaration`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_declaration
///
/// # Compatibility
///
/// This request was introduced in specification version 3.14.0.
///
/// The [`GotoDeclarationResponse::Link`](lsp_types::GotoDefinitionResponse::Link) return value
/// was introduced in specification version 3.14.0 and requires client-side
/// support in order to be used. It can be returned if the client set the
/// following field to `true` in the `initialize` method:
///
/// ```text
/// InitializeParams::capabilities::text_document::declaration::link_support
/// ```
#[derive(Debug, Clone)]
pub struct GotoDeclarationRequest {
    /// The path of the document to get the declaration location for.
    pub path: PathBuf,
    /// The position of the symbol to get the declaration location for.
    pub position: LspPosition,
}

impl SemanticRequest for GotoDeclarationRequest {
    type Response = GotoDeclarationResponse;

    fn request(self, ctx: &mut AnalysisContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let offset = ctx.to_typst_pos(self.position, &source)?;
        let cursor = offset + 1;

        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        debug!("ast_node: {ast_node:?}", ast_node = ast_node);
        let deref_target = get_deref_target(ast_node, cursor)?;

        let use_site = deref_target.node();
        let origin_selection_range = ctx.to_lsp_range(use_site.range(), &source);

        let def_use = ctx.def_use(source.clone())?;
        let ref_spans = find_declarations(ctx, def_use, deref_target)?;

        let mut links = vec![];
        for ref_range in ref_spans {
            let ref_id = source.id();
            let ref_source = &source;

            let span_path = ctx.path_for_id(ref_id).ok()?;
            let range = ctx.to_lsp_range(ref_range, ref_source);

            let uri = path_to_url(&span_path).ok()?;

            links.push(LocationLink {
                origin_selection_range: Some(origin_selection_range),
                target_uri: uri,
                target_range: range,
                target_selection_range: range,
            });
        }

        debug!("goto_declaration: {links:?}");
        Some(GotoDeclarationResponse::Link(links))
    }
}

fn find_declarations(
    _ctx: &AnalysisContext,
    _def_use: Arc<crate::analysis::DefUseInfo>,
    _deref_target: DerefTarget<'_>,
) -> Option<Vec<Range<usize>>> {
    todo!()
}
