use crate::prelude::*;

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

impl SemanticRequest for SemanticTokensFullRequest {
    type Response = SemanticTokensResult;

    /// Handles the request to compute the semantic tokens for a given document.
    fn request(self, ctx: &mut AnalysisContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let ei = ctx.expr_stage(&source);
        let token_ctx = &ctx.analysis.tokens_ctx;
        let (tokens, result_id) = token_ctx.semantic_tokens_full(&source, ei);

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

    /// This is converted by Copilot from TypeScript `to_multiline_tokens2`.
    /// <https://github.com/microsoft/vscode/blob/2acc0e52cbc7434c415f221d5c34ee1bbdd6cd71/src/vs/editor/common/services/semanticTokensProviderStyling.ts#L147>
    fn check_tokens(tokens: &SemanticTokens) {
        const DESIRED_TOKENS_PER_AREA: usize = 400;
        const DESIRED_MAX_AREAS: usize = 1024;

        let src_data = &tokens.data;
        let token_count = src_data.len();
        let tokens_per_area = std::cmp::max(
            (token_count as f64 / DESIRED_MAX_AREAS as f64).ceil() as usize,
            DESIRED_TOKENS_PER_AREA,
        );

        let mut token_index = 0;
        let mut last_line_number = 1;
        let mut last_start_character = 0;

        while token_index < token_count {
            let token_start_index = token_index;
            let mut token_end_index =
                std::cmp::min(token_start_index + tokens_per_area, token_count);

            // Keep tokens on the same line in the same area...
            if token_end_index < token_count {
                let mut small_token_end_index = token_end_index;
                while small_token_end_index - 1 > token_start_index
                    && src_data[small_token_end_index].delta_line == 0
                {
                    small_token_end_index -= 1;
                }

                if small_token_end_index - 1 == token_start_index {
                    // there are so many tokens on this line that our area would be empty, we must
                    // now go right
                    let mut big_token_end_index = token_end_index;
                    while big_token_end_index + 1 < token_count
                        && src_data[big_token_end_index].delta_line == 0
                    {
                        big_token_end_index += 1;
                    }
                    token_end_index = big_token_end_index;
                } else {
                    token_end_index = small_token_end_index;
                }
            }

            let mut prev_line_number = 0;
            let mut prev_end_character = 0;

            while token_index < token_end_index {
                let delta_line = src_data[token_index].delta_line;
                let delta_character = src_data[token_index].delta_start;
                let length = src_data[token_index].length;
                let line_number = last_line_number + delta_line;
                let start_character = if delta_line == 0 {
                    last_start_character + delta_character
                } else {
                    delta_character
                };
                // delta_character
                let end_character = start_character + length;

                if end_character <= start_character {
                    // this token is invalid (most likely a negative length casted to uint32)
                    panic!(
                        "Invalid length for semantic token at line {line_number}, character {start_character}, end: {end_character}"
                    );
                } else if prev_line_number == line_number && prev_end_character > start_character {
                    // this token overlaps with the previous token
                    panic!("Overlapping semantic tokens at line {line_number}, character {start_character}, previous line {prev_line_number}, previous end {prev_end_character}");
                } else {
                    prev_line_number = line_number;
                    prev_end_character = end_character;
                }

                last_line_number = line_number;
                last_start_character = start_character;
                token_index += 1;
            }
        }
    }

    #[test]
    fn test() {
        snapshot_testing("semantic_tokens", &|ctx, path| {
            let request = SemanticTokensFullRequest { path: path.clone() };

            let mut result = request.request(ctx).unwrap();
            if let SemanticTokensResult::Tokens(tokens) = &mut result {
                tokens.result_id.take();
            }

            match &result {
                SemanticTokensResult::Tokens(tokens) => {
                    check_tokens(tokens);
                }
                SemanticTokensResult::Partial(_) => {
                    panic!("Unexpected partial result");
                }
            }

            assert_snapshot!(serde_json::to_string(&result).unwrap());
        });
    }
}
