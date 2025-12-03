//! Common types and interfaces for the conversion system

use cmark_writer::HtmlAttribute;
use cmark_writer::HtmlElement;
use cmark_writer::HtmlWriteResult;
use cmark_writer::HtmlWriter;
use cmark_writer::HtmlWriterOptions;
use cmark_writer::WriteResult;
use cmark_writer::ast::Node;
use cmark_writer::custom_node;
use cmark_writer::writer::{BlockWriterProxy, InlineWriterProxy};
use ecow::EcoString;
use ecow::eco_format;
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
    /// The caption content for the figure
    pub caption: Vec<Node>,
}

impl FigureNode {
    fn write_custom(&self, writer: &mut BlockWriterProxy) -> WriteResult<()> {
        let content = writer.capture_block(|block| {
            block.write_block(&self.body)?;
            Ok(())
        })?;
        writer.write_str(&content)?;
        if !self.caption.is_empty() {
            writer.write_str("\n")?;
            writer.write_inline_nodes(&self.caption)?;
        }
        Ok(())
    }

    fn write_html_custom(&self, writer: &mut HtmlWriter) -> HtmlWriteResult<()> {
        let body = self.body.clone();
        let mut children = vec![*body];
        if !self.caption.is_empty() {
            children.extend(self.caption.clone());
        }
        let node = Node::HtmlElement(HtmlElement {
            tag: EcoString::inline("figure"),
            attributes: vec![HtmlAttribute {
                name: EcoString::inline("class"),
                value: EcoString::inline("figure"),
            }],
            children,
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
    pub alt_text: EcoString,
    /// Original SVG data (needed for DOCX that still embeds images)
    pub svg: String,
}

impl ExternalFrameNode {
    fn write_custom(&self, writer: &mut BlockWriterProxy) -> WriteResult<()> {
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
            tag: EcoString::inline("img"),
            attributes: vec![
                HtmlAttribute {
                    name: EcoString::inline("src"),
                    value: self.file_path.display().to_string().into(),
                },
                HtmlAttribute {
                    name: EcoString::inline("alt"),
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
    fn write_custom(&self, writer: &mut InlineWriterProxy) -> WriteResult<()> {
        writer.write_str("==")?;
        writer.write_inline_nodes(&self.content)?;
        writer.write_str("==")?;
        Ok(())
    }

    fn write_html_custom(&self, writer: &mut HtmlWriter) -> HtmlWriteResult<()> {
        let node = Node::HtmlElement(HtmlElement {
            tag: EcoString::inline("mark"),
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
                tag: EcoString::inline("p"),
                attributes: vec![cmark_writer::ast::HtmlAttribute {
                    name: EcoString::inline("align"),
                    value: EcoString::inline("center"),
                }],
                children,
                self_closing: false,
            }),
        }
    }

    fn write_custom(&self, writer: &mut BlockWriterProxy) -> WriteResult<()> {
        let content = writer.capture_inline(|inline| {
            inline.write_inline(&self.node)?;
            Ok(())
        })?;
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
        let content = temp_writer.into_string()?;
        writer.write_trusted_html(&content)?;
        Ok(())
    }
}

/// Inline node for flattened inline content (useful for table cells)
#[derive(Debug, PartialEq, Clone)]
#[custom_node(block = false, html_impl = true)]
pub struct InlineNode {
    /// The inline content nodes
    pub content: Vec<Node>,
}

impl InlineNode {
    fn write_custom(&self, writer: &mut InlineWriterProxy) -> WriteResult<()> {
        writer.write_inline_nodes(&self.content)
    }

    fn write_html_custom(&self, writer: &mut HtmlWriter) -> HtmlWriteResult<()> {
        for node in &self.content {
            writer.write_node(node)?;
        }
        Ok(())
    }
}

/// Verbatim node for raw text output
#[derive(Debug, PartialEq, Clone)]
#[custom_node(block = false, html_impl = true)]
pub struct VerbatimNode {
    /// The content to directly output
    pub content: EcoString,
}

impl VerbatimNode {
    fn write_custom(&self, writer: &mut InlineWriterProxy) -> WriteResult<()> {
        writer.write_str(&self.content)
    }

    fn write_html_custom(&self, writer: &mut HtmlWriter) -> HtmlWriteResult<()> {
        writer.write_trusted_html(&self.content)
    }
}

/// Alert node for alert messages
#[derive(Debug, PartialEq, Clone)]
#[custom_node(block = true, html_impl = false)]
pub struct AlertNode {
    /// The content of the alert
    pub content: Vec<Node>,
    /// The class of the alert
    pub class: EcoString,
}

impl AlertNode {
    fn write_custom(&self, writer: &mut BlockWriterProxy) -> WriteResult<()> {
        let quote = Node::BlockQuote(vec![
            Node::Paragraph(vec![Node::Text(eco_format!(
                "[!{}]",
                self.class.to_ascii_uppercase()
            ))]),
            Node::Paragraph(vec![Node::Text("".into())]),
        ]);
        writer.with_temporary_options(
            |options| options.escape_special_chars = false,
            |writer| writer.write_block(&quote),
        )?;
        let quote = Node::BlockQuote(self.content.clone());
        writer.write_block(&quote)?;
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
