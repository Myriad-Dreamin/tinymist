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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("semantic_tokens", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let request = SemanticTokensFullRequest { path: path.clone() };

            let cache = SemanticTokenCache::default();

            let mut result = request
                .request(&cache, source, PositionEncoding::Utf16)
                .unwrap();
            if let SemanticTokensResult::Tokens(tokens) = &mut result {
                tokens.result_id.take();
            }

            assert_snapshot!(HashRepr(JsonRepr::new_pure(result)));
        });
    }
}
