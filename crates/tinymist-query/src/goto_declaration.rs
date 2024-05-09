use std::ops::Range;

use log::debug;

use crate::{prelude::*, syntax::DerefTarget, SemanticRequest};

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
        let deref_target = ctx.deref_syntax_at(&source, self.position, 1)?;
        let origin_selection_range = ctx.to_lsp_range(deref_target.node().range(), &source);

        let def_use = ctx.def_use(source.clone())?;
        let ref_spans = find_declarations(ctx, def_use, deref_target)?;

        let mut links = vec![];
        for ref_range in ref_spans {
            let uri = ctx.uri_for_id(source.id()).ok()?;
            let range = ctx.to_lsp_range(ref_range, &source);

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
