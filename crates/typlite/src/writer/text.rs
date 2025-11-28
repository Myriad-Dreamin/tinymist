//! Text writer implementation - produces plain text output

use cmark_writer::ast::Node;
use ecow::EcoString;

use crate::Result;
use crate::common::{ExternalFrameNode, FigureNode, FormatWriter};

/// Text writer implementation
#[derive(Default)]
pub struct TextWriter {}

impl TextWriter {
    pub fn new() -> Self {
        Self {}
    }

    fn write_node(node: &Node, output: &mut EcoString) -> Result<()> {
        match node {
            Node::Document(blocks) => {
                for block in blocks {
                    Self::write_node(block, output)?;
                    output.push_str("\n");
                }
            }
            Node::Paragraph(inlines) => {
                for inline in inlines {
                    Self::write_node(inline, output)?;
                }
                output.push_str("\n");
            }
            Node::Heading {
                level: _,
                content,
                heading_type: _,
            } => {
                for inline in content {
                    Self::write_node(inline, output)?;
                }
                output.push_str("\n");
            }
            Node::BlockQuote(content) => {
                for block in content {
                    Self::write_node(block, output)?;
                }
            }
            Node::CodeBlock {
                language: _,
                content,
                block_type: _,
            } => {
                output.push_str(content);
                output.push_str("\n\n");
            }
            Node::OrderedList { start: _, items } => {
                for item in items.iter() {
                    match item {
                        cmark_writer::ast::ListItem::Ordered { content, .. }
                        | cmark_writer::ast::ListItem::Unordered { content } => {
                            for block in content {
                                Self::write_node(block, output)?;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Node::UnorderedList(items) => {
                for item in items {
                    match item {
                        cmark_writer::ast::ListItem::Ordered { content, .. }
                        | cmark_writer::ast::ListItem::Unordered { content } => {
                            for block in content {
                                Self::write_node(block, output)?;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Node::Table { rows, .. } => {
                for row in rows {
                    for cell in &row.cells {
                        Self::write_node(&cell.content, output)?;
                        output.push(' ');
                    }
                    output.push('\n');
                }
                output.push('\n');
            }
            Node::Text(text) => {
                output.push_str(text);
            }
            Node::Emphasis(content) | Node::Strong(content) | Node::Strikethrough(content) => {
                for inline in content {
                    Self::write_node(inline, output)?;
                }
            }
            Node::Link {
                url: _,
                title: _,
                content,
            } => {
                for inline in content {
                    Self::write_node(inline, output)?;
                }
            }
            Node::Image {
                url: _,
                title: _,
                alt,
            } => {
                if !alt.is_empty() {
                    for inline in alt {
                        Self::write_node(inline, output)?;
                    }
                }
            }
            Node::InlineCode(code) => {
                output.push_str(code);
            }
            Node::HardBreak => {
                output.push_str("\n");
            }
            Node::SoftBreak => {
                output.push(' ');
            }
            Node::ThematicBreak => {
                output.push_str("\n");
            }
            Node::HtmlElement(element) => {
                for child in &element.children {
                    Self::write_node(child, output)?;
                }
            }
            node if node.is_custom_type::<FigureNode>() => {
                if let Some(figure_node) = node.as_custom_type::<FigureNode>() {
                    Self::write_node(&figure_node.body, output)?;
                    if !figure_node.caption.is_empty() {
                        output.push_str("\n");
                        for inline in &figure_node.caption {
                            Self::write_node(inline, output)?;
                        }
                    }
                }
            }
            node if node.is_custom_type::<ExternalFrameNode>() => {
                if let Some(external_frame) = node.as_custom_type::<ExternalFrameNode>()
                    && !external_frame.alt_text.is_empty()
                {
                    output.push_str(&external_frame.alt_text);
                }
            }
            node if node.is_custom_type::<crate::common::HighlightNode>() => {
                if let Some(highlight) = node.as_custom_type::<crate::common::HighlightNode>() {
                    for child in &highlight.content {
                        Self::write_node(child, output)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl FormatWriter for TextWriter {
    fn write_eco(&mut self, document: &Node, output: &mut EcoString) -> Result<()> {
        Self::write_node(document, output)
    }

    fn write_vec(&mut self, document: &Node) -> Result<Vec<u8>> {
        let mut output = EcoString::new();
        Self::write_node(document, &mut output)?;
        Ok(output.as_str().as_bytes().to_vec())
    }
}
