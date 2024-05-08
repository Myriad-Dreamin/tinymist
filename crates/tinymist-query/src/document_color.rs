use crate::{analysis::get_color_exprs, prelude::*, SemanticRequest};

/// The [`textDocument/documentColor`] request is sent from the client to the
/// server to list all color references found in a given text document. Along
/// with the range, a color value in RGB is returned.
///
/// [`textDocument/documentColor`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_documentColor
///
/// Clients can use the result to decorate color references in an editor. For
/// example:
///
/// * Color boxes showing the actual color next to the reference
/// * Show a color picker when a color reference is edited
///
/// # Compatibility
///
/// This request was introduced in specification version 3.6.0.
#[derive(Debug, Clone)]
pub struct DocumentColorRequest {
    /// The path of the document to request color for.
    pub path: PathBuf,
}

impl SemanticRequest for DocumentColorRequest {
    type Response = Vec<ColorInformation>;

    fn request(self, ctx: &mut AnalysisContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        get_color_exprs(ctx, &source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("document_color", &|ctx, path| {
            let request = DocumentColorRequest { path: path.clone() };

            let result = request.request(ctx);
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
