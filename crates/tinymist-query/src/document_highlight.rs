use typst_shim::syntax::LinkedNodeExt;

use crate::{analysis::doc_highlight::DocumentHighlightWorker, prelude::*, SemanticRequest};

/// The [`textDocument/documentHighlight`] request
///
/// [`textDocument/documentHighlight`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_documentHighlight
#[derive(Debug, Clone)]
pub struct DocumentHighlightRequest {
    /// The path of the document to request highlight for.
    pub path: PathBuf,
    /// The position of the document to request highlight for.
    pub position: LspPosition,
}

impl SemanticRequest for DocumentHighlightRequest {
    type Response = Vec<DocumentHighlight>;

    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let cursor = ctx.to_typst_pos(self.position, &source)?;

        let root = LinkedNode::new(source.root());
        let node = root.leaf_at_compat(cursor)?;

        let mut worker = DocumentHighlightWorker::new(ctx, &source);
        worker.work(&node)?;
        (!worker.annotated.is_empty()).then_some(worker.annotated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("document_highlight", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let request = DocumentHighlightRequest {
                path: path.clone(),
                position: find_test_position_after(&source),
            };

            let result = request.request(ctx);
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
