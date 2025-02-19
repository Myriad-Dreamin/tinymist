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

    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let links = get_link_exprs(&source);
        if links.objects.is_empty() {
            return None;
        }

        let links = links.objects.iter().map(|obj| DocumentLink {
            range: ctx.to_lsp_range(obj.range.clone(), &source),
            target: obj.target.resolve(ctx),
            tooltip: None,
            data: None,
        });
        Some(links.collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("document_link", &|ctx, path| {
            let request = DocumentLinkRequest { path: path.clone() };

            let result = request.request(ctx);
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
