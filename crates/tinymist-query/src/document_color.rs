use crate::{analysis::get_color_exprs, prelude::*, SemanticRequest};

/// The [`textDocument/documentSymbol`] request is sent from the client to the
/// server to retrieve all symbols found in a given text document.
///
/// [`textDocument/documentSymbol`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_documentSymbol
///
/// The returned result is either:
///
/// * [`DocumentSymbolResponse::Flat`] which is a flat list of all symbols found
///   in a given text document. Then neither the symbol’s location range nor the
///   symbol’s container name should be used to infer a hierarchy.
/// * [`DocumentSymbolResponse::Nested`] which is a hierarchy of symbols found
///   in a given text document.
#[derive(Debug, Clone)]
pub struct DocumentColorRequest {
    /// The path of the document to retrieve symbols from.
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
