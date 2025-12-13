//! Parser implementation for Typst HTML to typlite IR.

mod core;
mod inline;
mod list;
mod media;
mod table;

pub use core::HtmlToIrParser;
