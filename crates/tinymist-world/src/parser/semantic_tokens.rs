//! From <https://github.com/nvarner/typst-lsp/blob/cc7bad9bd9764bfea783f2fab415cb3061fd8bff/src/server/semantic_tokens/mod.rs>

use strum::IntoEnumIterator;
use typst::syntax::{ast, LinkedNode, Source, SyntaxKind};

use super::modifier_set::ModifierSet;
use super::typst_tokens::{Modifier, TokenType};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct SemanticTokensLegend {
    #[serde(rename = "tokenTypes")]
    pub token_types: Vec<String>,
    #[serde(rename = "tokenModifiers")]
    pub token_modifiers: Vec<String>,
}

pub fn get_semantic_tokens_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TokenType::iter()
            .map(|e| {
                let e: &'static str = e.into();

                e.to_owned()
            })
            .collect(),
        token_modifiers: Modifier::iter()
            .map(|e| {
                let e: &'static str = e.into();

                e.to_owned()
            })
            .collect(),
    }
}

#[derive(Debug, Clone, Copy)]
pub enum OffsetEncoding {
    Utf8,
    Utf16,
}

pub fn get_semantic_tokens_full(source: &Source, encoding: OffsetEncoding) -> Vec<SemanticToken> {
    let root = LinkedNode::new(source.root());
    let mut full = tokenize_tree(&root, ModifierSet::empty());

    let mut init = (0, 0);
    for token in full.iter_mut() {
        // resolve offset to position
        let offset = ((token.delta_line as u64) << 32) | token.delta_start_character as u64;
        let position = (match encoding {
            OffsetEncoding::Utf8 => offset_to_position_utf8,
            OffsetEncoding::Utf16 => offset_to_position_utf16,
        })(offset as usize, source);
        token.delta_line = position.0;
        token.delta_start_character = position.1;

        let next = (token.delta_line, token.delta_start_character);
        token.delta_line -= init.0;
        if token.delta_line == 0 {
            token.delta_start_character -= init.1;
        }
        init = next;
    }

    full
}

fn tokenize_single_node(node: &LinkedNode, modifiers: ModifierSet) -> Option<SemanticToken> {
    let is_leaf = node.children().next().is_none();

    token_from_node(node)
        .or_else(|| is_leaf.then_some(TokenType::Text))
        .map(|token_type| SemanticToken::new(token_type, modifiers, node))
}

/// Tokenize a node and its children
fn tokenize_tree(root: &LinkedNode<'_>, parent_modifiers: ModifierSet) -> Vec<SemanticToken> {
    let modifiers = parent_modifiers | modifiers_from_node(root);

    let token = tokenize_single_node(root, modifiers).into_iter();
    let children = root
        .children()
        .flat_map(move |child| tokenize_tree(&child, modifiers));
    token.chain(children).collect()
}

#[derive(Debug, Clone, Copy)]
pub struct SemanticToken {
    pub delta_line: u32,
    pub delta_start_character: u32,
    pub length: u32,
    pub token_type: u32,
    pub token_modifiers: u32,
}

impl SemanticToken {
    fn new(token_type: TokenType, modifiers: ModifierSet, node: &LinkedNode) -> Self {
        let source = node.get().clone().into_text();

        let raw_position = node.offset() as u64;
        let raw_position = ((raw_position >> 32) as u32, raw_position as u32);

        Self {
            token_type: token_type as u32,
            token_modifiers: modifiers.bitset(),
            delta_line: raw_position.0,
            delta_start_character: raw_position.1,
            length: source.chars().map(char::len_utf16).sum::<usize>() as u32,
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
        MathAlignPoint | Plus | Minus | Slash | Hat | Dot | Eq | EqEq | ExclEq | Lt | LtEq | Gt
        | GtEq | PlusEq | HyphEq | StarEq | SlashEq | Dots | Arrow | Not | And | Or => {
            Some(TokenType::Operator)
        }
        Dollar => Some(TokenType::Delimiter),
        None | Auto | Let | Show | If | Else | For | In | While | Break | Continue | Return
        | Import | Include | As | Set => Some(TokenType::Keyword),
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
        .filter(|next| next.cast::<ast::Expr>().is_some_and(|expr| expr.hash()))
        .and_then(|node| node.leftmost_leaf())
}

fn token_from_hashtag(hashtag: &LinkedNode) -> Option<TokenType> {
    get_expr_following_hashtag(hashtag)
        .as_ref()
        .and_then(token_from_node)
}

fn offset_to_position_utf8(typst_offset: usize, typst_source: &Source) -> (u32, u32) {
    let line_index = typst_source.byte_to_line(typst_offset).unwrap();
    let column_index = typst_source.byte_to_column(typst_offset).unwrap();

    (line_index as u32, column_index as u32)
}

fn offset_to_position_utf16(typst_offset: usize, typst_source: &Source) -> (u32, u32) {
    let line_index = typst_source.byte_to_line(typst_offset).unwrap();

    let lsp_line = line_index as u32;

    // See the implementation of `lsp_to_typst::position_to_offset` for discussion
    // relevant to this function.

    // TODO: Typst's `Source` could easily provide an implementation of the method
    // we   need here. Submit a PR to `typst` to add it, then update
    // this if/when merged.

    let utf16_offset = typst_source.byte_to_utf16(typst_offset).unwrap();

    let byte_line_offset = typst_source.line_to_byte(line_index).unwrap();
    let utf16_line_offset = typst_source.byte_to_utf16(byte_line_offset).unwrap();

    let utf16_column_offset = utf16_offset - utf16_line_offset;
    let lsp_column = utf16_column_offset;

    (lsp_line, lsp_column as u32)
}
