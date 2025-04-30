//! Parser implementation for Typst HTML to CommonMark AST

mod core;
mod inline;
mod list;
mod media;
mod table;

pub use core::HtmlToAstParser;

use cmark_writer::ast::Node;
use typst::html::HtmlElement;

use crate::Result;
use crate::TypliteFeat;

/// Generic parser trait for converting HTML to AST
pub trait Parser {
    /// Parse HTML element to AST
    fn parse(&self, source: &HtmlElement) -> Result<Node>;
}

/// Create a new parser instance
pub fn create_parser(feat: TypliteFeat) -> impl Parser {
    HtmlToAstParser::new(feat)
}
