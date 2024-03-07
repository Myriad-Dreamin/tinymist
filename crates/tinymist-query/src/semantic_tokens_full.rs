use crate::{prelude::*, SemanticTokenCache};

#[derive(Debug, Clone)]
pub struct SemanticTokensFullRequest {
    pub path: PathBuf,
}

impl SemanticTokensFullRequest {
    pub fn request(
        self,
        cache: &SemanticTokenCache,
        source: Source,
        position_encoding: PositionEncoding,
    ) -> Option<SemanticTokensResult> {
        let (tokens, result_id) = cache.get_semantic_tokens_full(&source, position_encoding);

        Some(
            SemanticTokens {
                result_id: Some(result_id),
                data: tokens,
            }
            .into(),
        )
    }
}
