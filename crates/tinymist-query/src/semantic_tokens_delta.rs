use crate::{prelude::*, SemanticTokenContext};

#[derive(Debug, Clone)]
pub struct SemanticTokensDeltaRequest {
    pub path: PathBuf,
    pub previous_result_id: String,
}

impl SemanticTokensDeltaRequest {
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
