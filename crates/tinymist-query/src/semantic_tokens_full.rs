use crate::{prelude::*, SemanticTokenCache};

#[derive(Debug, Clone)]
pub struct SemanticTokensFullRequest {
    pub path: PathBuf,
}

pub fn semantic_tokens_full(
    cache: &SemanticTokenCache,
    source: Source,
    _req: SemanticTokensFullRequest,
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
