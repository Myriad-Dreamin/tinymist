#![allow(missing_docs)]

mod modifier_set;
mod semantic_tokens;
mod typst_tokens;

pub use semantic_tokens::{
    OffsetEncoding, SemanticToken, SemanticTokensLegend, get_semantic_tokens_full,
    get_semantic_tokens_legend,
};
