use std::ops::Range;

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

    fn request(self, _ctx: &mut AnalysisContext) -> Option<Self::Response> {
        let _ = find_declarations;
        todo!()
    }
}

fn find_declarations(
    _ctx: &AnalysisContext,
    _expr_info: Arc<crate::syntax::ExprInfo>,
    _deref_target: DerefTarget<'_>,
) -> Option<Vec<Range<usize>>> {
    todo!()
}
