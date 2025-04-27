//! Converter implementations for different output formats

mod docx;
mod latex;
mod markdown;

use cmark_writer::WriteResult;
pub use docx::DocxConverter;
pub use latex::LaTeXConverter;
pub use markdown::MarkdownConverter;

use cmark_writer::ast::{CustomNodeWriter, Node};
use cmark_writer::derive_custom_node;
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

/// Figure node implementation for all formats
#[derive(Debug, PartialEq, Clone)]
pub struct FigureNode {
    /// The main content of the figure, can be any block node
    pub body: Box<Node>,
    /// The caption text for the figure
    pub caption: String,
}

derive_custom_node!(FigureNode);
impl FigureNode {
    fn write_custom(&self, writer: &mut dyn CustomNodeWriter) -> WriteResult<()> {
        let mut temp_writer = cmark_writer::writer::CommonMarkWriter::new();
        temp_writer.write(&self.body)?;
        let content = temp_writer.into_string();
        writer.write_str(&content)?;
        writer.write_str("\n")?;
        writer.write_str(&self.caption)?;
        Ok(())
    }

    fn is_block_custom(&self) -> bool {
        true
    }
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
