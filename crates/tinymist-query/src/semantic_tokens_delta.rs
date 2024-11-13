use crate::{prelude::*, semantic_tokens_delta};

/// The [`textDocument/semanticTokens/full/delta`] request is sent from the
/// client to the server to resolve the semantic tokens of a given file,
/// **returning only the delta**.
///
/// [`textDocument/semanticTokens/full/delta`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_semanticTokens
///
/// Similar to [`semantic_tokens_full`](crate::SemanticTokensFullRequest),
/// except it returns a sequence of [`lsp_types::SemanticTokensEdit`] to
/// transform a previous result into a new result.
///
/// # Compatibility
///
/// This request was introduced in specification version 3.16.0.
#[derive(Debug, Clone)]
pub struct SemanticTokensDeltaRequest {
    /// The path of the document to get semantic tokens for.
    pub path: PathBuf,
    /// The previous result id to compute the delta from.
    pub previous_result_id: String,
}

impl SemanticRequest for SemanticTokensDeltaRequest {
    type Response = SemanticTokensFullDeltaResult;
    /// Handles the request to compute the semantic tokens delta for a given
    /// document.
    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let ei = ctx.expr_stage(&source);

        let (tokens, result_id) = semantic_tokens_delta(ctx, &source, ei, &self.previous_result_id);

        match tokens {
            Ok(edits) => Some(
                SemanticTokensDelta {
                    result_id: Some(result_id),
                    edits,
                }
                .into(),
            ),
            Err(tokens) => Some(
                SemanticTokens {
                    result_id: Some(result_id),
                    data: tokens,
                }
                .into(),
            ),
        }
    }
}
