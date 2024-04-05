use crate::{analysis::get_color_exprs, prelude::*, SemanticRequest};

/// The
#[derive(Debug, Clone)]
pub struct DocumentColorRequest {
    /// The.
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
