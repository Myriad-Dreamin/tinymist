//! Converter implementations for different output formats

mod docx;
mod latex;
mod markdown;

pub use docx::DocxConverter;
pub use latex::LaTeXConverter;
pub use markdown::MarkdownConverter;

use cmark_writer::ast::Node;
use ecow::EcoString;
use typst::html::HtmlElement;

use crate::Result;
use crate::TypliteFeat;

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

/// Common HTML to AST parser for all converters
pub struct HtmlToAstParser {
    feat: TypliteFeat,
}

impl HtmlToAstParser {
    pub fn new(feat: TypliteFeat) -> Self {
        Self { feat }
    }

    /// Parse HTML structure to CommonMark AST
    pub fn parse(&self, root: &HtmlElement) -> Result<Node> {
        // 使用 MarkdownConverter 的实现来转换 HTML 到 AST
        // 但不生成 markdown 输出
        let mut converter = markdown::MarkdownConverter::new(self.feat.clone());
        let blocks = Vec::new();
        let inline_buffer = Vec::new();
        converter.blocks = blocks;
        converter.inline_buffer = inline_buffer;
        converter.convert_element(root)?;
        converter.flush_inline_buffer();
        
        Ok(Node::Document(converter.blocks.clone()))
    }
}

/// Common writer interface for different formats
pub trait FormatWriter {
    /// Write AST document to output format
    fn write_eco(&mut self, document: &Node, output: &mut EcoString) -> Result<()>;

    /// Write AST document to vector
    fn write_vec(&mut self, document: &Node) -> Result<Vec<u8>>;
}
