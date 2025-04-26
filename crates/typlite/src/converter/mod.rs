//! Converter implementations for different output formats

mod docx;
mod latex;
mod markdown;

use cmark_writer::WriteResult;
pub use docx::DocxConverter;
pub use latex::LaTeXConverter;
pub use markdown::MarkdownConverter;

use cmark_writer::ast::{CustomNode, CustomNodeWriter, Node};
use ecow::EcoString;
use std::any::Any;
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

impl CustomNode for FigureNode {
    fn write(&self, writer: &mut dyn CustomNodeWriter) -> WriteResult<()> {
        // For Markdown, we'll represent a figure as:
        // <figure>
        // [content - typically an image]
        //
        // <figcaption>Caption text</figcaption>
        // </figure>

        // Start the figure element
        writer.write_str("<figure>\n")?;

        // Write the body node content
        match &*self.body {
            Node::Paragraph(content) => {
                for node in content {
                    self.write_node(node, writer)?;
                }
                writer.write_str("\n")?;
            }
            node => self.write_node(node, writer)?,
        }

        // Add the caption
        if !self.caption.is_empty() {
            writer.write_str("<figcaption>")?;
            writer.write_str(&self.caption)?;
            writer.write_str("</figcaption>\n")?;
        }

        // Close the figure element
        writer.write_str("</figure>")?;

        Ok(())
    }

    fn clone_box(&self) -> Box<dyn CustomNode> {
        Box::new(Self {
            body: self.body.clone(),
            caption: self.caption.clone(),
        })
    }

    fn eq_box(&self, other: &dyn CustomNode) -> bool {
        if let Some(other) = other.as_any().downcast_ref::<FigureNode>() {
            self == other
        } else {
            false
        }
    }

    fn is_block(&self) -> bool {
        true // Figure is always a block-level element
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl FigureNode {
    // Helper method to write a node to the provided writer
    fn write_node(&self, node: &Node, writer: &mut dyn CustomNodeWriter) -> WriteResult<()> {
        let mut temp_writer = cmark_writer::writer::CommonMarkWriter::new();
        temp_writer.write(node)?;
        let content = temp_writer.into_string();
        writer.write_str(&content)?;
        Ok(())
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
