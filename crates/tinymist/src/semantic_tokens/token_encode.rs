use tower_lsp::lsp_types::{Position, SemanticToken};
use typst::diag::EcoString;
use typst::syntax::Source;

use crate::config::PositionEncoding;
use crate::ext::{PositionExt, StrExt};
use crate::lsp_typst_boundary::typst_to_lsp;

use super::Token;

pub(super) fn encode_tokens<'a>(
    tokens: impl Iterator<Item = Token> + 'a,
    source: &'a Source,
    encoding: PositionEncoding,
) -> impl Iterator<Item = (SemanticToken, EcoString)> + 'a {
    tokens.scan(Position::new(0, 0), move |last_position, token| {
        let (encoded_token, source_code, position) =
            encode_token(token, last_position, source, encoding);
        *last_position = position;
        Some((encoded_token, source_code))
    })
}

fn encode_token(
    token: Token,
    last_position: &Position,
    source: &Source,
    encoding: PositionEncoding,
) -> (SemanticToken, EcoString, Position) {
    let position = typst_to_lsp::offset_to_position(token.offset, encoding, source);
    let delta = last_position.delta(&position);

    let length = token.source.as_str().encoded_len(encoding);

    let lsp_token = SemanticToken {
        delta_line: delta.delta_line,
        delta_start: delta.delta_start,
        length: length as u32,
        token_type: token.token_type as u32,
        token_modifiers_bitset: token.modifiers.bitset(),
    };

    (lsp_token, token.source, position)
}
