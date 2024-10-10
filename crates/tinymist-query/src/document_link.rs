use crate::{analysis::get_link_exprs, prelude::*, SemanticRequest};

/// The [`textDocument/documentLink`] request is sent from the client to the
/// server to request the location of links in a document.
///
/// A document link is a range in a text document that links to an internal or
/// external resource, like another text document or a web site.
///
/// [`textDocument/documentLink`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_documentLink
///
/// # Compatibility
///
/// The [`DocumentLink::tooltip`] field was introduced in specification version
/// 3.15.0 and requires client-side support in order to be used.
#[derive(Debug, Clone)]
pub struct DocumentLinkRequest {
    /// The path of the document to request color for.
    pub path: PathBuf,
}

impl SemanticRequest for DocumentLinkRequest {
    type Response = Vec<DocumentLink>;

    fn request(self, ctx: &mut AnalysisContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let links = get_link_exprs(ctx, &source);
        links.map(|links| {
            links
                .into_iter()
                .map(|(range, target)| DocumentLink {
                    range: ctx.to_lsp_range(range, &source),
                    target: Some(target),
                    tooltip: None,
                    data: None,
                })
                .collect()
        })
    }
}
