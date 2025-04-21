//! Converter implementations for different output formats

mod latex;
mod markdown;

pub use latex::LaTeXConverter;
pub use markdown::MarkdownConverter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListState {
    Ordered,
    Unordered,
}

/// Valid formats for the conversion.
pub enum Format {
    Md,
    LaTeX,
}
