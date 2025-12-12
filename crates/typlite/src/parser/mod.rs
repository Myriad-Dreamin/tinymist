//! Parser implementation for Typst HTML to typlite IR.
//!
//! `HtmlToIrParser` is the primary parser. `HtmlToAstParser` is kept as a
//! thin compatibility wrapper for existing callers.

mod core;
mod inline;
mod list;
mod media;
mod table;

pub use core::{HtmlToAstParser, HtmlToIrParser};
