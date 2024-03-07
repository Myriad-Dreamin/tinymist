use tower_lsp::lsp_types::{Position, SemanticToken};
use typst::diag::EcoString;
use typst::syntax::Source;

use crate::typst_to_lsp;
use crate::PositionEncoding;

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

pub trait StrExt {
    fn encoded_len(&self, encoding: PositionEncoding) -> usize;
}

impl StrExt for str {
    fn encoded_len(&self, encoding: PositionEncoding) -> usize {
        match encoding {
            PositionEncoding::Utf8 => self.len(),
            PositionEncoding::Utf16 => self.chars().map(char::len_utf16).sum(),
        }
    }
}

pub trait PositionExt {
    fn delta(&self, to: &Self) -> PositionDelta;
}

impl PositionExt for Position {
    /// Calculates the delta from `self` to `to`. This is in the `SemanticToken`
    /// sense, so the delta's `character` is relative to `self`'s
    /// `character` iff `self` and `to` are on the same line. Otherwise,
    /// it's relative to the start of the line `to` is on.
    fn delta(&self, to: &Self) -> PositionDelta {
        let line_delta = to.line - self.line;
        let char_delta = if line_delta == 0 {
            to.character - self.character
        } else {
            to.character
        };

        PositionDelta {
            delta_line: line_delta,
            delta_start: char_delta,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Default)]
pub struct PositionDelta {
    pub delta_line: u32,
    pub delta_start: u32,
}
