use lsp_types::{SemanticToken, SemanticTokensEdit};

use crate::{get_semantic_tokens, prelude::*};

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
        let (tokens, result_id) = get_semantic_tokens(ctx, &source, ei);

        let (tokens, result_id) = match ctx.tokens.as_ref().and_then(|t| t.previous()) {
            Some(cached) => (Ok(token_delta(cached, &tokens)), result_id),
            None => {
                log::warn!(
                    "No previous tokens found for delta computation in {}, prev_id: {:?}",
                    self.path.display(),
                    self.previous_result_id
                );
                (Err(tokens), result_id)
            }
        };

        match tokens {
            Ok(edits) => Some(SemanticTokensDelta { result_id, edits }.into()),
            Err(tokens) => Some(
                SemanticTokens {
                    result_id,
                    data: tokens.as_ref().clone(),
                }
                .into(),
            ),
        }
    }
}

fn token_delta(from: &[SemanticToken], to: &[SemanticToken]) -> Vec<SemanticTokensEdit> {
    // Taken from `rust-analyzer`'s algorithm
    // https://github.com/rust-lang/rust-analyzer/blob/master/crates/rust-analyzer/src/semantic_tokens.rs#L219

    let start = from
        .iter()
        .zip(to.iter())
        .take_while(|(x, y)| x == y)
        .count();

    let (_, from) = from.split_at(start);
    let (_, to) = to.split_at(start);

    let dist_from_end = from
        .iter()
        .rev()
        .zip(to.iter().rev())
        .take_while(|(x, y)| x == y)
        .count();

    let (from, _) = from.split_at(from.len() - dist_from_end);
    let (to, _) = to.split_at(to.len() - dist_from_end);

    if from.is_empty() && to.is_empty() {
        vec![]
    } else {
        vec![SemanticTokensEdit {
            start: 5 * start as u32,
            delete_count: 5 * from.len() as u32,
            data: Some(to.into()),
        }]
    }
}
