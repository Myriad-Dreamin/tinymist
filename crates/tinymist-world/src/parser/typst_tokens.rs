//! Types for tokens used for Typst syntax

use strum::EnumIter;

/// Very similar to [`typst_ide::Tag`], but with convenience traits, and
/// extensible because we want to further customize highlighting
#[derive(Debug, Clone, Copy, EnumIter)]
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
}

impl From<TokenType> for &'static str {
    fn from(token_type: TokenType) -> Self {
        use TokenType::*;

        match token_type {
            Comment => "comment",
            String => "string",
            Keyword => "keyword",
            Operator => "operator",
            Number => "number",
            Function => "function",
            Decorator => "decorator",
            Bool => "bool",
            Punctuation => "punctuation",
            Escape => "escape",
            Link => "link",
            Raw => "raw",
            Label => "label",
            Ref => "ref",
            Heading => "heading",
            ListMarker => "marker",
            ListTerm => "term",
            Delimiter => "delim",
            Interpolated => "pol",
            Error => "error",
            Text => "text",
        }
    }
}

#[derive(Debug, Clone, Copy, EnumIter)]
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

impl From<Modifier> for &'static str {
    fn from(modifier: Modifier) -> Self {
        use Modifier::*;

        match modifier {
            Strong => "strong",
            Emph => "emph",
            Math => "math",
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
