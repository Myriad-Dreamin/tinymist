use crate::{prelude::*, SemanticTokenContext};

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

impl SemanticTokensDeltaRequest {
    /// Handles the request to compute the semantic tokens delta for a given
    /// document.
    pub fn request(
        self,
        ctx: &SemanticTokenContext,
        source: Source,
    ) -> Option<SemanticTokensFullDeltaResult> {
        let (tokens, result_id) =
            ctx.try_semantic_tokens_delta_from_result_id(&source, &self.previous_result_id);

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
