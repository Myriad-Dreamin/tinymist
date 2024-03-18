use crate::{prelude::*, SemanticTokenContext};

#[derive(Debug, Clone)]
pub struct SemanticTokensFullRequest {
    pub path: PathBuf,
}

impl SemanticTokensFullRequest {
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
