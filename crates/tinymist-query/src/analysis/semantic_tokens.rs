//! Semantic tokens (highlighting) support for LSP.

use std::{
    num::NonZeroUsize,
    ops::Range,
    path::Path,
    sync::{Arc, OnceLock},
};

use hashbrown::HashMap;
use lsp_types::SemanticToken;
use lsp_types::{SemanticTokenModifier, SemanticTokenType};
use parking_lot::Mutex;
use strum::EnumIter;
use tinymist_std::ImmutPath;
use typst::syntax::{ast, LinkedNode, Source, SyntaxKind};

use crate::{
    adt::revision::{RevisionLock, RevisionManager, RevisionManagerLike, RevisionSlot},
    syntax::{Expr, ExprInfo},
    ty::Ty,
    LocalContext, LspPosition, PositionEncoding,
};

/// A shared semantic tokens object.
pub type SemanticTokens = Arc<Vec<SemanticToken>>;

/// Get the semantic tokens for a source.
pub(crate) fn get_semantic_tokens(ctx: &mut LocalContext, source: &Source) -> SemanticTokens {
    let mut tokenizer = Tokenizer::new(
        source.clone(),
        ctx.expr_stage(source),
        ctx.analysis.allow_multiline_token,
        ctx.analysis.position_encoding,
    );
    tokenizer.tokenize_tree(&LinkedNode::new(source.root()), ModifierSet::empty());
    SemanticTokens::new(tokenizer.output)
}

/// A shared semantic tokens cache.
#[derive(Default)]
pub struct SemanticTokenCache {
    next_id: usize,
    // todo: clear cache after didClose
    manager: HashMap<ImmutPath, RevisionManager<OnceLock<SemanticTokens>>>,
}

impl SemanticTokenCache {
    pub(crate) fn clear(&mut self) {
        self.next_id = 0;
        self.manager.clear();
    }

    /// Lock the token cache with an optional previous id in *main thread*.
    pub(crate) fn acquire(
        cache: Arc<Mutex<Self>>,
        path: &Path,
        prev: Option<&str>,
    ) -> SemanticTokenContext {
        let that = cache.clone();
        let mut that = that.lock();

        that.next_id += 1;
        let prev = prev.and_then(|id| {
            id.parse::<NonZeroUsize>()
                .inspect_err(|_| {
                    log::warn!("invalid previous id: {id}");
                })
                .ok()
        });
        let next = NonZeroUsize::new(that.next_id).expect("id overflow");

        let path = ImmutPath::from(path);
        let manager = that.manager.entry(path.clone()).or_default();
        let _rev_lock = manager.lock(prev.unwrap_or(next));
        let prev = prev.and_then(|prev| {
            manager
                .find_revision(prev, |_| OnceLock::new())
                .data
                .get()
                .cloned()
        });
        let next = manager.find_revision(next, |_| OnceLock::new());

        SemanticTokenContext {
            _rev_lock,
            cache,
            path,
            prev,
            next,
        }
    }
}

/// A semantic token context providing incremental semantic tokens rendering.
pub(crate) struct SemanticTokenContext {
    _rev_lock: RevisionLock,
    cache: Arc<Mutex<SemanticTokenCache>>,
    path: ImmutPath,
    pub prev: Option<SemanticTokens>,
    pub next: Arc<RevisionSlot<OnceLock<SemanticTokens>>>,
}

impl Drop for SemanticTokenContext {
    fn drop(&mut self) {
        let mut cache = self.cache.lock();
        let manager = cache.manager.get_mut(&self.path);
        if let Some(manager) = manager {
            let min_rev = manager.unlock(&mut self._rev_lock);
            if let Some(min_rev) = min_rev {
                manager.gc(min_rev);
            }
        }
    }
}

const BOOL: SemanticTokenType = SemanticTokenType::new("bool");
const PUNCTUATION: SemanticTokenType = SemanticTokenType::new("punct");
const ESCAPE: SemanticTokenType = SemanticTokenType::new("escape");
const LINK: SemanticTokenType = SemanticTokenType::new("link");
const RAW: SemanticTokenType = SemanticTokenType::new("raw");
const LABEL: SemanticTokenType = SemanticTokenType::new("label");
const REF: SemanticTokenType = SemanticTokenType::new("ref");
const HEADING: SemanticTokenType = SemanticTokenType::new("heading");
const LIST_MARKER: SemanticTokenType = SemanticTokenType::new("marker");
const LIST_TERM: SemanticTokenType = SemanticTokenType::new("term");
const DELIMITER: SemanticTokenType = SemanticTokenType::new("delim");
const INTERPOLATED: SemanticTokenType = SemanticTokenType::new("pol");
const ERROR: SemanticTokenType = SemanticTokenType::new("error");
const TEXT: SemanticTokenType = SemanticTokenType::new("text");

/// Very similar to `typst_ide::Tag`, but with convenience traits, and
/// extensible because we want to further customize highlighting
#[derive(Clone, Copy, Eq, PartialEq, EnumIter, Default)]
#[repr(u32)]
pub enum TokenType {
    // Standard LSP types
    /// A comment token.
    Comment,
    /// A string token.
    String,
    /// A keyword token.
    Keyword,
    /// An operator token.
    Operator,
    /// A number token.
    Number,
    /// A function token.
    Function,
    /// A decorator token.
    Decorator,
    /// A type token.
    Type,
    /// A namespace token.
    Namespace,
    // Custom types
    /// A boolean token.
    Bool,
    /// A punctuation token.
    Punctuation,
    /// An escape token.
    Escape,
    /// A link token.
    Link,
    /// A raw token.
    Raw,
    /// A label token.
    Label,
    /// A markup reference token.
    Ref,
    /// A heading token.
    Heading,
    /// A list marker token.
    ListMarker,
    /// A list term token.
    ListTerm,
    /// A delimiter token.
    Delimiter,
    /// An interpolated token.
    Interpolated,
    /// An error token.
    Error,
    /// Any text in markup without a more specific token type, possible styled.
    ///
    /// We perform styling (like bold and italics) via modifiers. That means
    /// everything that should receive styling needs to be a token so we can
    /// apply a modifier to it. This token type is mostly for that, since
    /// text should usually not be specially styled.
    Text,
    /// A token that is not recognized by the lexer
    #[default]
    None,
}

impl From<TokenType> for SemanticTokenType {
    fn from(token_type: TokenType) -> Self {
        use TokenType::*;

        match token_type {
            Comment => Self::COMMENT,
            String => Self::STRING,
            Keyword => Self::KEYWORD,
            Operator => Self::OPERATOR,
            Number => Self::NUMBER,
            Function => Self::FUNCTION,
            Decorator => Self::DECORATOR,
            Type => Self::TYPE,
            Namespace => Self::NAMESPACE,
            Bool => BOOL,
            Punctuation => PUNCTUATION,
            Escape => ESCAPE,
            Link => LINK,
            Raw => RAW,
            Label => LABEL,
            Ref => REF,
            Heading => HEADING,
            ListMarker => LIST_MARKER,
            ListTerm => LIST_TERM,
            Delimiter => DELIMITER,
            Interpolated => INTERPOLATED,
            Error => ERROR,
            Text => TEXT,
            None => unreachable!(),
        }
    }
}

const STRONG: SemanticTokenModifier = SemanticTokenModifier::new("strong");
const EMPH: SemanticTokenModifier = SemanticTokenModifier::new("emph");
const MATH: SemanticTokenModifier = SemanticTokenModifier::new("math");

/// A modifier to some semantic token.
#[derive(Clone, Copy, EnumIter)]
#[repr(u8)]
pub enum Modifier {
    /// Strong modifier.
    Strong,
    /// Emphasis modifier.
    Emph,
    /// Math modifier.
    Math,
    /// Read-only modifier.
    ReadOnly,
    /// Static modifier.
    Static,
    /// Default library modifier.
    DefaultLibrary,
}

impl Modifier {
    /// Get the index of the modifier.
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// Get the bitmask of the modifier.
    pub const fn bitmask(self) -> u32 {
        0b1 << self.index()
    }
}

impl From<Modifier> for SemanticTokenModifier {
    fn from(modifier: Modifier) -> Self {
        use Modifier::*;

        match modifier {
            Strong => STRONG,
            Emph => EMPH,
            Math => MATH,
            ReadOnly => Self::READONLY,
            Static => Self::STATIC,
            DefaultLibrary => Self::DEFAULT_LIBRARY,
        }
    }
}

#[derive(Default, Clone, Copy)]
pub(crate) struct ModifierSet(u32);

impl ModifierSet {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn new(modifiers: &[Modifier]) -> Self {
        let bits = modifiers
            .iter()
            .copied()
            .map(Modifier::bitmask)
            .fold(0, |bits, mask| bits | mask);
        Self(bits)
    }

    pub fn bitset(self) -> u32 {
        self.0
    }
}

impl std::ops::BitOr for ModifierSet {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

pub(crate) struct Tokenizer {
    curr_pos: LspPosition,
    pos_offset: usize,
    output: Vec<SemanticToken>,
    source: Source,
    ei: Arc<ExprInfo>,
    encoding: PositionEncoding,

    allow_multiline_token: bool,

    token: Option<Token>,
}

impl Tokenizer {
    pub fn new(
        source: Source,
        ei: Arc<ExprInfo>,
        allow_multiline_token: bool,
        encoding: PositionEncoding,
    ) -> Self {
        Self {
            curr_pos: LspPosition::new(0, 0),
            pos_offset: 0,
            output: Vec::new(),
            source,
            ei,
            allow_multiline_token,
            encoding,

            token: None,
        }
    }

    /// Tokenize a node and its children
    fn tokenize_tree(&mut self, root: &LinkedNode, modifiers: ModifierSet) {
        let is_leaf = root.get().children().len() == 0;
        let mut modifiers = modifiers | modifiers_from_node(root);

        let range = root.range();
        let mut token = token_from_node(&self.ei, root, &mut modifiers)
            .or_else(|| is_leaf.then_some(TokenType::Text))
            .map(|token_type| Token::new(token_type, modifiers, range.clone()));

        // Push start
        if let Some(prev_token) = self.token.as_mut() {
            if !prev_token.range.is_empty() && prev_token.range.start < range.start {
                let end = prev_token.range.end.min(range.start);
                let sliced = Token {
                    token_type: prev_token.token_type,
                    modifiers: prev_token.modifiers,
                    range: prev_token.range.start..end,
                };
                // Slice the previous token
                prev_token.range.start = end;
                self.push(sliced);
            }
        }

        if !is_leaf {
            std::mem::swap(&mut self.token, &mut token);
            for child in root.children() {
                self.tokenize_tree(&child, modifiers);
            }
            std::mem::swap(&mut self.token, &mut token);
        }

        // Push end
        if let Some(token) = token.clone() {
            if !token.range.is_empty() {
                // Slice the previous token
                if let Some(prev_token) = self.token.as_mut() {
                    prev_token.range.start = token.range.end;
                }
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

        use crate::lsp_typst_boundary;
        use lsp_types::Position;
        let utf8_start = range.start;
        if self.pos_offset > utf8_start {
            return;
        }

        // This might be a bug of typst, that `end > len` is possible
        let source_len = self.source.text().len();
        let utf8_end = (range.end).min(source_len);
        self.pos_offset = utf8_start;
        if utf8_end <= utf8_start || utf8_start > source_len {
            return;
        }

        let position = lsp_typst_boundary::to_lsp_position(utf8_start, self.encoding, &self.source);

        let delta = self.curr_pos.delta(&position);

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
            self.curr_pos = position;
        } else {
            let final_line = self
                .source
                .byte_to_line(utf8_end)
                .unwrap_or_else(|| self.source.len_lines()) as u32;
            let next_offset = self
                .source
                .line_to_byte((self.curr_pos.line + 1) as usize)
                .unwrap_or(source_len);
            let inline_length = encode_length(utf8_start, utf8_end.min(next_offset)) as u32;
            if inline_length != 0 {
                self.output.push(SemanticToken {
                    delta_line: delta.delta_line,
                    delta_start: delta.delta_start,
                    length: inline_length,
                    token_type: token_type as u32,
                    token_modifiers_bitset: modifiers.bitset(),
                });
                self.curr_pos = position;
            }
            if self.curr_pos.line >= final_line {
                return;
            }

            let mut utf8_cursor = next_offset;
            let mut delta_line = 0;
            for line in self.curr_pos.line + 1..=final_line {
                let next_offset = if line == final_line {
                    utf8_end
                } else {
                    self.source
                        .line_to_byte((line + 1) as usize)
                        .unwrap_or(source_len)
                };

                if utf8_cursor < next_offset {
                    let inline_length = encode_length(utf8_cursor, next_offset) as u32;
                    self.output.push(SemanticToken {
                        delta_line: delta_line + 1,
                        delta_start: 0,
                        length: inline_length,
                        token_type: token_type as u32,
                        token_modifiers_bitset: modifiers.bitset(),
                    });
                    delta_line = 0;
                    self.curr_pos.character = 0;
                } else {
                    delta_line += 1;
                }
                self.pos_offset = utf8_cursor;
                utf8_cursor = next_offset;
            }
            self.curr_pos.line = final_line - delta_line;
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
fn token_from_node(
    ei: &ExprInfo,
    node: &LinkedNode,
    modifier: &mut ModifierSet,
) -> Option<TokenType> {
    use SyntaxKind::*;

    match node.kind() {
        Star if node.parent_kind() == Some(Strong) => Some(TokenType::Punctuation),
        Star if node.parent_kind() == Some(ModuleImport) => Some(TokenType::Operator),

        Underscore if node.parent_kind() == Some(Emph) => Some(TokenType::Punctuation),
        Underscore if node.parent_kind() == Some(MathAttach) => Some(TokenType::Operator),

        MathIdent | Ident => Some(token_from_ident(ei, node, modifier)),
        Hash => token_from_hashtag(ei, node, modifier),

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
fn token_from_ident(ei: &ExprInfo, ident: &LinkedNode, modifier: &mut ModifierSet) -> TokenType {
    let resolved = ei.resolves.get(&ident.span());
    let context = if let Some(resolved) = resolved {
        match (&resolved.root, &resolved.term) {
            (Some(root), term) => Some(token_from_decl_expr(root, term.as_ref(), modifier)),
            (_, Some(ty)) => Some(token_from_term(ty, modifier)),
            _ => None,
        }
    } else {
        None
    };

    if !matches!(context, None | Some(TokenType::Interpolated)) {
        return context.unwrap_or(TokenType::Interpolated);
    }

    let next = ident.next_leaf();
    let next_is_adjacent = next
        .as_ref()
        .is_some_and(|n| n.range().start == ident.range().end);
    let next_parent = next.as_ref().and_then(|n| n.parent_kind());
    let next_kind = next.map(|n| n.kind());
    let lexical_function_call = next_is_adjacent
        && matches!(next_kind, Some(SyntaxKind::LeftParen))
        && matches!(next_parent, Some(SyntaxKind::Args | SyntaxKind::Params));
    if lexical_function_call {
        return TokenType::Function;
    }

    let function_content = next_is_adjacent
        && matches!(next_kind, Some(SyntaxKind::LeftBracket))
        && matches!(next_parent, Some(SyntaxKind::ContentBlock));
    if function_content {
        return TokenType::Function;
    }

    TokenType::Interpolated
}

fn token_from_term(t: &Ty, modifier: &mut ModifierSet) -> TokenType {
    use typst::foundations::Value::*;
    match t {
        Ty::Func(..) => TokenType::Function,
        Ty::Value(v) => {
            match &v.val {
                Func(..) => TokenType::Function,
                Type(..) => {
                    *modifier = *modifier | ModifierSet::new(&[Modifier::DefaultLibrary]);
                    TokenType::Function
                }
                Module(..) => ns(modifier),
                // todo: read only modifier
                _ => TokenType::Interpolated,
            }
        }
        _ => TokenType::Interpolated,
    }
}

fn token_from_decl_expr(expr: &Expr, term: Option<&Ty>, modifier: &mut ModifierSet) -> TokenType {
    use crate::syntax::Decl::*;
    match expr {
        Expr::Type(term) => token_from_term(term, modifier),
        Expr::Decl(decl) => match decl.as_ref() {
            Func(..) => TokenType::Function,
            Var(..) => TokenType::Interpolated,
            Module(..) => ns(modifier),
            ModuleAlias(..) => ns(modifier),
            PathStem(..) => ns(modifier),
            ImportAlias(..) => TokenType::Interpolated,
            IdentRef(..) => TokenType::Interpolated,
            ImportPath(..) => TokenType::Interpolated,
            IncludePath(..) => TokenType::Interpolated,
            Import(..) => TokenType::Interpolated,
            ContentRef(..) => TokenType::Interpolated,
            Label(..) => TokenType::Interpolated,
            StrName(..) => TokenType::Interpolated,
            ModuleImport(..) => TokenType::Interpolated,
            Closure(..) => TokenType::Interpolated,
            Pattern(..) => TokenType::Interpolated,
            Spread(..) => TokenType::Interpolated,
            Content(..) => TokenType::Interpolated,
            Constant(..) => TokenType::Interpolated,
            BibEntry(..) => TokenType::Interpolated,
            Docs(..) => TokenType::Interpolated,
            Generated(..) => TokenType::Interpolated,
        },
        _ => term
            .map(|term| token_from_term(term, modifier))
            .unwrap_or(TokenType::Interpolated),
    }
}

fn ns(modifier: &mut ModifierSet) -> TokenType {
    *modifier = *modifier | ModifierSet::new(&[Modifier::Static, Modifier::ReadOnly]);
    TokenType::Namespace
}

fn get_expr_following_hashtag<'a>(hashtag: &LinkedNode<'a>) -> Option<LinkedNode<'a>> {
    hashtag
        .next_sibling()
        .filter(|next| next.cast::<ast::Expr>().is_some_and(|expr| expr.hash()))
        .and_then(|node| node.leftmost_leaf())
}

fn token_from_hashtag(
    ei: &ExprInfo,
    hashtag: &LinkedNode,
    modifier: &mut ModifierSet,
) -> Option<TokenType> {
    get_expr_following_hashtag(hashtag)
        .as_ref()
        .and_then(|node| token_from_node(ei, node, modifier))
}

#[cfg(test)]
mod tests {
    use strum::IntoEnumIterator;

    use super::*;

    #[test]
    fn ensure_not_too_many_modifiers() {
        // Because modifiers are encoded in a 32 bit bitmask, we can't have more than 32
        // modifiers
        assert!(Modifier::iter().len() <= 32);
    }
}
