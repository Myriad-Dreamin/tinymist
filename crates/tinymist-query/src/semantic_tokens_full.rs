use crate::{prelude::*, SemanticTokenContext};

/// The [`textDocument/semanticTokens/full`] request is sent from the client to
/// the server to resolve the semantic tokens of a given file.
///
/// [`textDocument/semanticTokens/full`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_semanticTokens
///
/// Semantic tokens are used to add additional color information to a file that
/// depends on language specific symbol information. A semantic token request
/// usually produces a large result. The protocol therefore supports encoding
/// tokens with numbers. In addition, optional support for deltas is available,
/// i.e. [`semantic_tokens_full_delta`].
///
/// [`semantic_tokens_full_delta`]: crate::SemanticTokensDeltaRequest
///
/// # Compatibility
///
/// This request was introduced in specification version 3.16.0.
#[derive(Debug, Clone)]
pub struct SemanticTokensFullRequest {
    /// The path of the document to get semantic tokens for.
    pub path: PathBuf,
}

impl SemanticTokensFullRequest {
    /// Handles the request to compute the semantic tokens for a given document.
    pub fn request(
        self,
        ctx: &SemanticTokenContext,
        source: Source,
    ) -> Option<SemanticTokensResult> {
        let (tokens, result_id) = ctx.get_semantic_tokens_full(&source);

        Some(
            SemanticTokens {
                result_id: Some(result_id),
                data: tokens,
            }
            .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("semantic_tokens", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let request = SemanticTokensFullRequest { path: path.clone() };

            let cache = SemanticTokenContext::default();

            let mut result = request.request(&cache, source).unwrap();
            if let SemanticTokensResult::Tokens(tokens) = &mut result {
                tokens.result_id.take();
            }

            assert_snapshot!(serde_json::to_string(&result).unwrap());
        });
    }
}
