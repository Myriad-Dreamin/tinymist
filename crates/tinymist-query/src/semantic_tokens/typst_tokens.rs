//! Types for tokens used for Typst syntax

use lsp_types::{SemanticTokenModifier, SemanticTokenType};
use strum::EnumIter;

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

/// Very similar to [`typst_ide::Tag`], but with convenience traits, and
/// extensible because we want to further customize highlighting
#[derive(Clone, Copy, Eq, PartialEq, EnumIter, Default)]
#[repr(u32)]
pub enum TokenType {
    // Standard LSP types
    Comment,
    String,
    Keyword,
    Operator,
    Number,
    Function,
    Decorator,
    // Custom types
    Bool,
    Punctuation,
    Escape,
    Link,
    Raw,
    Label,
    Ref,
    Heading,
    ListMarker,
    ListTerm,
    Delimiter,
    Interpolated,
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

#[derive(Clone, Copy, EnumIter)]
#[repr(u8)]
pub enum Modifier {
    Strong,
    Emph,
    Math,
}

impl Modifier {
    pub fn index(self) -> u8 {
        self as u8
    }

    pub fn bitmask(self) -> u32 {
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
        }
    }
}

#[cfg(test)]
mod test {
    use strum::IntoEnumIterator;

    use super::*;

    #[test]
    fn ensure_not_too_many_modifiers() {
        // Because modifiers are encoded in a 32 bit bitmask, we can't have more than 32
        // modifiers
        assert!(Modifier::iter().len() <= 32);
    }
}
