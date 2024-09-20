use std::ops::Range;

use lsp_types::{SemanticToken, SemanticTokensEdit};
use parking_lot::RwLock;
use typst::syntax::{ast, LinkedNode, Source, SyntaxKind};

use crate::{LspPosition, PositionEncoding};

use self::delta::token_delta;
use self::modifier_set::ModifierSet;

use self::delta::CacheInner as TokenCacheInner;

mod delta;
mod modifier_set;
mod typst_tokens;
pub use self::typst_tokens::{Modifier, TokenType};

/// A semantic token context providing incremental semantic tokens rendering.
#[derive(Default)]
pub struct SemanticTokenContext {
    cache: RwLock<TokenCacheInner>,
    position_encoding: PositionEncoding,
    /// Whether to allow overlapping tokens.
    pub allow_overlapping_token: bool,
    /// Whether to allow multiline tokens.
    pub allow_multiline_token: bool,
}

impl SemanticTokenContext {
    /// Create a new semantic token context.
    pub fn new(
        position_encoding: PositionEncoding,
        allow_overlapping_token: bool,
        allow_multiline_token: bool,
    ) -> Self {
        Self {
            cache: RwLock::new(TokenCacheInner::default()),
            position_encoding,
            allow_overlapping_token,
            allow_multiline_token,
        }
    }

    /// Get the semantic tokens for a source.
    pub fn get_semantic_tokens_full(&self, source: &Source) -> (Vec<SemanticToken>, String) {
        let root = LinkedNode::new(source.root());

        let mut tokenizer = Tokenizer::new(
            source.clone(),
            self.allow_multiline_token,
            self.position_encoding,
        );
        tokenizer.tokenize_tree(&root, ModifierSet::empty());
        let output = tokenizer.output;

        let result_id = self.cache.write().cache_result(output.clone());
        (output, result_id)
    }

    /// Get the semantic tokens delta for a source.
    pub fn try_semantic_tokens_delta_from_result_id(
        &self,
        source: &Source,
        result_id: &str,
    ) -> (Result<Vec<SemanticTokensEdit>, Vec<SemanticToken>>, String) {
        let cached = self.cache.write().try_take_result(result_id);

        // this call will overwrite the cache, so need to read from cache first
        let (tokens, result_id) = self.get_semantic_tokens_full(source);

        match cached {
            Some(cached) => (Ok(token_delta(&cached, &tokens)), result_id),
            None => (Err(tokens), result_id),
        }
    }
}

struct Tokenizer {
    curr_pos: LspPosition,
    pos_offset: usize,
    output: Vec<SemanticToken>,
    source: Source,
    encoding: PositionEncoding,

    allow_multiline_token: bool,

    token: Token,
}

impl Tokenizer {
    fn new(source: Source, allow_multiline_token: bool, encoding: PositionEncoding) -> Self {
        Self {
            curr_pos: LspPosition::new(0, 0),
            pos_offset: 0,
            output: Vec::new(),
            source,
            allow_multiline_token,
            encoding,

            token: Token::default(),
        }
    }

    /// Tokenize a node and its children
    fn tokenize_tree(&mut self, root: &LinkedNode, modifiers: ModifierSet) {
        let is_leaf = root.get().children().len() == 0;
        let modifiers = modifiers | modifiers_from_node(root);

        let range = root.range();
        let mut token = token_from_node(root)
            .or_else(|| is_leaf.then_some(TokenType::Text))
            .map(|token_type| Token::new(token_type, modifiers, range.clone()));

        // Push start
        if !self.token.range.is_empty() && self.token.range.start < range.start {
            let end = self.token.range.end.min(range.start);
            self.push(Token {
                token_type: self.token.token_type,
                modifiers: self.token.modifiers,
                range: self.token.range.start..end,
            });
            self.token.range.start = end;
        }

        if !is_leaf {
            if let Some(token) = token.as_mut() {
                std::mem::swap(&mut self.token, token);
            }

            for child in root.children() {
                self.tokenize_tree(&child, modifiers);
            }

            if let Some(token) = token.as_mut() {
                std::mem::swap(&mut self.token, token);
            }
        }

        // Push end
        if let Some(token) = token.clone() {
            if !token.range.is_empty() {
                self.push(token);
            }
        }
    }

    fn push(&mut self, token: Token) {
        let Token {
            token_type,
            modifiers,
            range,
        } = token;

        use crate::typst_to_lsp;
        use lsp_types::Position;
        let utf8_start = range.start;
        if self.pos_offset > utf8_start {
            return;
        }

        // This might be a bug of typst, that `end > len` is possible
        let utf8_end = (range.end).min(self.source.text().len());
        self.pos_offset = utf8_start;
        if utf8_end < range.start || range.start > self.source.text().len() {
            return;
        }

        let position = typst_to_lsp::offset_to_position(utf8_start, self.encoding, &self.source);

        let delta = self.curr_pos.delta(&position);
        self.curr_pos = position;

        let encode_length = |s, t| {
            match self.encoding {
                PositionEncoding::Utf8 => t - s,
                PositionEncoding::Utf16 => {
                    // todo: whether it is safe to unwrap
                    let utf16_start = self.source.byte_to_utf16(s).unwrap();
                    let utf16_end = self.source.byte_to_utf16(t).unwrap();
                    utf16_end - utf16_start
                }
            }
        };

        if self.allow_multiline_token {
            self.output.push(SemanticToken {
                delta_line: delta.delta_line,
                delta_start: delta.delta_start,
                length: encode_length(utf8_start, utf8_end) as u32,
                token_type: token_type as u32,
                token_modifiers_bitset: modifiers.bitset(),
            });
        } else {
            let final_line = self
                .source
                .byte_to_line(utf8_end)
                .unwrap_or_else(|| self.source.len_lines()) as u32;
            let next_offset = self
                .source
                .line_to_byte((self.curr_pos.line + 1) as usize)
                .unwrap_or(self.source.text().len());
            self.output.push(SemanticToken {
                delta_line: delta.delta_line,
                delta_start: delta.delta_start,
                length: encode_length(utf8_start, utf8_end.min(next_offset)) as u32,
                token_type: token_type as u32,
                token_modifiers_bitset: modifiers.bitset(),
            });
            let mut utf8_cursor = next_offset;
            if self.curr_pos.line < final_line {
                for line in self.curr_pos.line + 1..=final_line {
                    let next_offset = if line == final_line {
                        utf8_end
                    } else {
                        self.source
                            .line_to_byte((line + 1) as usize)
                            .unwrap_or(self.source.text().len())
                    };

                    self.output.push(SemanticToken {
                        delta_line: 1,
                        delta_start: 0,
                        length: encode_length(utf8_cursor, next_offset) as u32,
                        token_type: token_type as u32,
                        token_modifiers_bitset: modifiers.bitset(),
                    });
                    self.pos_offset = utf8_cursor;
                    utf8_cursor = next_offset;
                }
                self.curr_pos.line = final_line;
                self.curr_pos.character = 0;
            }
        }

        pub trait PositionExt {
            fn delta(&self, to: &Self) -> PositionDelta;
        }

        impl PositionExt for Position {
            /// Calculates the delta from `self` to `to`. This is in the
            /// `SemanticToken` sense, so the delta's `character` is
            /// relative to `self`'s `character` iff `self` and `to`
            /// are on the same line. Otherwise, it's relative to
            /// the start of the line `to` is on.
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
    }
}

#[derive(Clone, Default)]
struct Token {
    pub token_type: TokenType,
    pub modifiers: ModifierSet,
    pub range: Range<usize>,
}

impl Token {
    pub fn new(token_type: TokenType, modifiers: ModifierSet, range: Range<usize>) -> Self {
        Self {
            token_type,
            modifiers,
            range,
        }
    }
}

/// Determines the [`Modifier`]s to be applied to a node and all its children.
///
/// Note that this does not recurse up, so calling it on a child node may not
/// return a modifier that should be applied to it due to a parent.
fn modifiers_from_node(node: &LinkedNode) -> ModifierSet {
    match node.kind() {
        SyntaxKind::Emph => ModifierSet::new(&[Modifier::Emph]),
        SyntaxKind::Strong => ModifierSet::new(&[Modifier::Strong]),
        SyntaxKind::Math | SyntaxKind::Equation => ModifierSet::new(&[Modifier::Math]),
        _ => ModifierSet::empty(),
    }
}

/// Determines the best [`TokenType`] for an entire node and its children, if
/// any. If there is no single `TokenType`, or none better than `Text`, returns
/// `None`.
///
/// In tokenization, returning `Some` stops recursion, while returning `None`
/// continues and attempts to tokenize each of `node`'s children. If there are
/// no children, `Text` is taken as the default.
fn token_from_node(node: &LinkedNode) -> Option<TokenType> {
    use SyntaxKind::*;

    match node.kind() {
        Star if node.parent_kind() == Some(Strong) => Some(TokenType::Punctuation),
        Star if node.parent_kind() == Some(ModuleImport) => Some(TokenType::Operator),

        Underscore if node.parent_kind() == Some(Emph) => Some(TokenType::Punctuation),
        Underscore if node.parent_kind() == Some(MathAttach) => Some(TokenType::Operator),

        MathIdent | Ident => Some(token_from_ident(node)),
        Hash => token_from_hashtag(node),

        LeftBrace | RightBrace | LeftBracket | RightBracket | LeftParen | RightParen | Comma
        | Semicolon | Colon => Some(TokenType::Punctuation),
        Linebreak | Escape | Shorthand => Some(TokenType::Escape),
        Link => Some(TokenType::Link),
        Raw => Some(TokenType::Raw),
        Label => Some(TokenType::Label),
        RefMarker => Some(TokenType::Ref),
        Heading | HeadingMarker => Some(TokenType::Heading),
        ListMarker | EnumMarker | TermMarker => Some(TokenType::ListMarker),
        Not | And | Or => Some(TokenType::Keyword),
        MathAlignPoint | Plus | Minus | Slash | Hat | Dot | Eq | EqEq | ExclEq | Lt | LtEq | Gt
        | GtEq | PlusEq | HyphEq | StarEq | SlashEq | Dots | Arrow => Some(TokenType::Operator),
        Dollar => Some(TokenType::Delimiter),
        None | Auto | Let | Show | If | Else | For | In | While | Break | Continue | Return
        | Import | Include | As | Set | Context => Some(TokenType::Keyword),
        Bool => Some(TokenType::Bool),
        Int | Float | Numeric => Some(TokenType::Number),
        Str => Some(TokenType::String),
        LineComment | BlockComment => Some(TokenType::Comment),
        Error => Some(TokenType::Error),

        // Disambiguate from `SyntaxKind::None`
        _ => Option::None,
    }
}

// TODO: differentiate also using tokens in scope, not just context
fn is_function_ident(ident: &LinkedNode) -> bool {
    let Some(next) = ident.next_leaf() else {
        return false;
    };
    let function_call = matches!(next.kind(), SyntaxKind::LeftParen)
        && matches!(
            next.parent_kind(),
            Some(SyntaxKind::Args | SyntaxKind::Params)
        );
    let function_content = matches!(next.kind(), SyntaxKind::LeftBracket)
        && matches!(next.parent_kind(), Some(SyntaxKind::ContentBlock));
    function_call || function_content
}

fn token_from_ident(ident: &LinkedNode) -> TokenType {
    if is_function_ident(ident) {
        TokenType::Function
    } else {
        TokenType::Interpolated
    }
}

fn get_expr_following_hashtag<'a>(hashtag: &LinkedNode<'a>) -> Option<LinkedNode<'a>> {
    hashtag
        .next_sibling()
        .filter(|next| next.cast::<ast::Expr>().map_or(false, |expr| expr.hash()))
        .and_then(|node| node.leftmost_leaf())
}

fn token_from_hashtag(hashtag: &LinkedNode) -> Option<TokenType> {
    get_expr_following_hashtag(hashtag)
        .as_ref()
        .and_then(token_from_node)
}
