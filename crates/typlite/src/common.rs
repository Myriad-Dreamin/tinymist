//! Common types and interfaces for the conversion system

use cmark_writer::ast::Node;
use cmark_writer::custom_node;
use cmark_writer::CommonMarkWriter;
use cmark_writer::HtmlAttribute;
use cmark_writer::HtmlElement;
use cmark_writer::HtmlWriteResult;
use cmark_writer::HtmlWriter;
use cmark_writer::HtmlWriterOptions;
use cmark_writer::WriteResult;
use cmark_writer::WriterOptions;
use ecow::EcoString;
use std::path::PathBuf;

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListState {
    Ordered,
    Unordered,
}

/// Valid formats for the conversion.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    #[default]
    Md,
    LaTeX,
    Text,
    #[cfg(feature = "docx")]
    Docx,
}

/// Figure node implementation for all formats
#[derive(Debug, PartialEq, Clone)]
#[custom_node(block = true, html_impl = true)]
pub struct FigureNode {
    /// The main content of the figure, can be any block node
    pub body: Box<Node>,
    /// The caption text for the figure
    pub caption: String,
}

impl FigureNode {
    fn write_custom(&self, writer: &mut CommonMarkWriter) -> WriteResult<()> {
        let mut temp_writer = CommonMarkWriter::with_options(WriterOptions {
            strict: false,
            ..Default::default()
        });
        temp_writer.write(&self.body)?;
        let content = temp_writer.into_string();
        writer.write_str(&content)?;
        writer.write_str("\n")?;
        writer.write_str(&self.caption)?;
        Ok(())
    }

    fn write_html_custom(&self, writer: &mut HtmlWriter) -> HtmlWriteResult<()> {
        let body = self.body.clone();
        let node = Node::HtmlElement(HtmlElement {
            tag: "figure".to_string(),
            attributes: vec![HtmlAttribute {
                name: "class".to_string(),
                value: "figure".to_string(),
            }],
            children: vec![*body],
            self_closing: false,
        });
        writer.write_node(&node)?;
        Ok(())
    }
}

/// External Frame node for handling frames stored as external files
#[derive(Debug, PartialEq, Clone)]
#[custom_node(block = true, html_impl = true)]
pub struct ExternalFrameNode {
    /// The path to the external file containing the frame
    pub file_path: PathBuf,
    /// Alternative text for the frame
    pub alt_text: String,
    /// Original SVG data (needed for DOCX that still embeds images)
    pub svg_data: String,
}

impl ExternalFrameNode {
    fn write_custom(&self, writer: &mut CommonMarkWriter) -> WriteResult<()> {
        // The actual handling is implemented in format-specific writers
        writer.write_str(&format!(
            "![{}]({})",
            self.alt_text,
            self.file_path.display()
        ))?;
        Ok(())
    }

    fn write_html_custom(&self, writer: &mut HtmlWriter) -> HtmlWriteResult<()> {
        let node = Node::HtmlElement(HtmlElement {
            tag: "img".to_string(),
            attributes: vec![
                HtmlAttribute {
                    name: "src".to_string(),
                    value: self.file_path.display().to_string(),
                },
                HtmlAttribute {
                    name: "alt".to_string(),
                    value: self.alt_text.clone(),
                },
            ],
            children: vec![],
            self_closing: true,
        });
        writer.write_node(&node)?;
        Ok(())
    }
}

/// Highlight node for highlighted text
#[derive(Debug, PartialEq, Clone)]
#[custom_node(block = false, html_impl = true)]
pub struct HighlightNode {
    /// The content to be highlighted
    pub content: Vec<Node>,
}

impl HighlightNode {
    fn write_custom(&self, writer: &mut CommonMarkWriter) -> WriteResult<()> {
        let mut temp_writer = CommonMarkWriter::with_options(WriterOptions {
            strict: false,
            ..Default::default()
        });
        for node in &self.content {
            temp_writer.write(node)?;
        }
        let content = temp_writer.into_string();
        writer.write_str(&format!("=={}==", content))?;
        Ok(())
    }

    fn write_html_custom(&self, writer: &mut HtmlWriter) -> HtmlWriteResult<()> {
        let node = Node::HtmlElement(HtmlElement {
            tag: "mark".to_string(),
            attributes: vec![],
            children: self.content.clone(),
            self_closing: false,
        });
        writer.write_node(&node)?;
        Ok(())
    }
}

/// Node for centered content
#[derive(Debug, PartialEq, Clone)]
#[custom_node(block = true, html_impl = true)]
pub struct CenterNode {
    /// The content to be centered
    pub node: Node,
}

impl CenterNode {
    pub fn new(children: Vec<Node>) -> Self {
        CenterNode {
            node: Node::HtmlElement(cmark_writer::ast::HtmlElement {
                tag: "p".to_string(),
                attributes: vec![cmark_writer::ast::HtmlAttribute {
                    name: "align".to_string(),
                    value: "center".to_string(),
                }],
                children,
                self_closing: false,
            }),
        }
    }

    fn write_custom(&self, writer: &mut CommonMarkWriter) -> WriteResult<()> {
        let mut temp_writer = CommonMarkWriter::with_options(WriterOptions {
            strict: false,
            ..Default::default()
        });
        temp_writer.write(&self.node)?;
        let content = temp_writer.into_string();
        writer.write_str(&content)?;
        writer.write_str("\n")?;
        Ok(())
    }

    fn write_html_custom(&self, writer: &mut HtmlWriter) -> HtmlWriteResult<()> {
        let mut temp_writer = HtmlWriter::with_options(HtmlWriterOptions {
            strict: false,
            ..Default::default()
        });
        temp_writer.write_node(&self.node)?;
        let content = temp_writer.into_string();
        writer.write_str(&content)?;
        Ok(())
    }
}

/// Common writer interface for different formats
pub trait FormatWriter {
    /// Write AST document to output format
    fn write_eco(&mut self, document: &Node, output: &mut EcoString) -> Result<()>;

    /// Write AST document to vector
    fn write_vec(&mut self, document: &Node) -> Result<Vec<u8>>;
}
