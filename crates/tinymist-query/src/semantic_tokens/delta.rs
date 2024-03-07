use tower_lsp::lsp_types::{SemanticToken, SemanticTokensEdit};

#[derive(Debug)]
struct CachedTokens {
    tokens: Vec<SemanticToken>,
    id: u64,
}

#[derive(Default, Debug)]
pub struct CacheInner {
    last_sent: Option<CachedTokens>,
    next_id: u64,
}

impl CacheInner {
    pub fn try_take_result(&mut self, id: &str) -> Option<Vec<SemanticToken>> {
        let id = id.parse::<u64>().ok()?;
        match self.last_sent.take() {
            Some(cached) if cached.id == id => Some(cached.tokens),
            Some(cached) => {
                // replace after taking
                self.last_sent = Some(cached);
                None
            }
            None => None,
        }
    }

    pub fn cache_result(&mut self, tokens: Vec<SemanticToken>) -> String {
        let id = self.get_next_id();
        let cached = CachedTokens { tokens, id };
        self.last_sent = Some(cached);
        id.to_string()
    }

    fn get_next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

pub fn token_delta(from: &[SemanticToken], to: &[SemanticToken]) -> Vec<SemanticTokensEdit> {
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
