use itertools::Itertools;
use parking_lot::RwLock;
use strum::IntoEnumIterator;
use tower_lsp::lsp_types::{
    Registration, SemanticToken, SemanticTokensEdit, SemanticTokensFullOptions,
    SemanticTokensLegend, SemanticTokensOptions, Unregistration,
};
use typst::diag::EcoString;
use typst::syntax::{ast, LinkedNode, Source, SyntaxKind};

use crate::PositionEncoding;

use self::delta::token_delta;
use self::modifier_set::ModifierSet;
use self::token_encode::encode_tokens;
use self::typst_tokens::{Modifier, TokenType};

pub use self::delta::CacheInner as TokenCacheInner;

mod delta;
mod modifier_set;
mod token_encode;
mod typst_tokens;

pub fn get_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TokenType::iter().map(Into::into).collect(),
        token_modifiers: Modifier::iter().map(Into::into).collect(),
    }
}

const SEMANTIC_TOKENS_REGISTRATION_ID: &str = "semantic_tokens";
const SEMANTIC_TOKENS_METHOD_ID: &str = "textDocument/semanticTokens";

pub fn get_semantic_tokens_registration(options: SemanticTokensOptions) -> Registration {
    Registration {
        id: SEMANTIC_TOKENS_REGISTRATION_ID.to_owned(),
        method: SEMANTIC_TOKENS_METHOD_ID.to_owned(),
        register_options: Some(
            serde_json::to_value(options)
                .expect("semantic tokens options should be representable as JSON value"),
        ),
    }
}

pub fn get_semantic_tokens_unregistration() -> Unregistration {
    Unregistration {
        id: SEMANTIC_TOKENS_REGISTRATION_ID.to_owned(),
        method: SEMANTIC_TOKENS_METHOD_ID.to_owned(),
    }
}

pub fn get_semantic_tokens_options() -> SemanticTokensOptions {
    SemanticTokensOptions {
        legend: get_legend(),
        full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
        ..Default::default()
    }
}

#[derive(Default)]
pub struct SemanticTokenCache(RwLock<TokenCacheInner>);

impl SemanticTokenCache {
    pub fn get_semantic_tokens_full(
        &self,
        source: &Source,
        encoding: PositionEncoding,
    ) -> (Vec<SemanticToken>, String) {
        let root = LinkedNode::new(source.root());

        let tokens = tokenize_tree(&root, ModifierSet::empty());
        let encoded_tokens = encode_tokens(tokens, source, encoding);
        let output_tokens = encoded_tokens.map(|(token, _)| token).collect_vec();

        let result_id = self.0.write().cache_result(output_tokens.clone());

        (output_tokens, result_id)
    }

    pub fn try_semantic_tokens_delta_from_result_id(
        &self,
        source: &Source,
        result_id: &str,
        encoding: PositionEncoding,
    ) -> (Result<Vec<SemanticTokensEdit>, Vec<SemanticToken>>, String) {
        let cached = self.0.write().try_take_result(result_id);

        // this call will overwrite the cache, so need to read from cache first
        let (tokens, result_id) = self.get_semantic_tokens_full(source, encoding);

        match cached {
            Some(cached) => (Ok(token_delta(&cached, &tokens)), result_id),
            None => (Err(tokens), result_id),
        }
    }
}

fn tokenize_single_node(node: &LinkedNode, modifiers: ModifierSet) -> Option<Token> {
    let is_leaf = node.children().next().is_none();

    token_from_node(node)
        .or_else(|| is_leaf.then_some(TokenType::Text))
        .map(|token_type| Token::new(token_type, modifiers, node))
}

/// Tokenize a node and its children
fn tokenize_tree<'a>(
    root: &LinkedNode<'a>,
    parent_modifiers: ModifierSet,
) -> Box<dyn Iterator<Item = Token> + 'a> {
    let modifiers = parent_modifiers | modifiers_from_node(root);

    let token = tokenize_single_node(root, modifiers).into_iter();
    let children = root
        .children()
        .flat_map(move |child| tokenize_tree(&child, modifiers));
    Box::new(token.chain(children))
}

pub struct Token {
    pub token_type: TokenType,
    pub modifiers: ModifierSet,
    pub offset: usize,
    pub source: EcoString,
}

impl Token {
    pub fn new(token_type: TokenType, modifiers: ModifierSet, node: &LinkedNode) -> Self {
        let source = node.get().clone().into_text();

        Self {
            token_type,
            modifiers,
            offset: node.offset(),
            source,
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
        .filter(|next| next.cast::<ast::Expr>().map_or(false, |expr| expr.hash()))
        .and_then(|node| node.leftmost_leaf())
}

fn token_from_hashtag(hashtag: &LinkedNode) -> Option<TokenType> {
    get_expr_following_hashtag(hashtag)
        .as_ref()
        .and_then(token_from_node)
}
