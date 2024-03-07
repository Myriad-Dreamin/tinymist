use crate::{prelude::*, SemanticTokenCache};

#[derive(Debug, Clone)]
pub struct SemanticTokensFullRequest {
    pub path: PathBuf,
    pub position_encoding: PositionEncoding,
}

pub fn semantic_tokens_full(
    cache: &SemanticTokenCache,
    source: Source,
    req: SemanticTokensFullRequest,
) -> Option<SemanticTokensResult> {
    let (tokens, result_id) = cache.get_semantic_tokens_full(&source, req.position_encoding);

    Some(
        SemanticTokens {
            result_id: Some(result_id),
            data: tokens,
        }
        .into(),
    )
}
