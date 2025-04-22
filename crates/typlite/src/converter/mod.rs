//! Converter implementations for different output formats

mod docx;
mod latex;
mod markdown;

pub use docx::DocxConverter;
pub use latex::LaTeXConverter;
pub use markdown::MarkdownConverter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListState {
    Ordered,
    Unordered,
}

/// Valid formats for the conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Md,
    LaTeX,
    Docx,
}
