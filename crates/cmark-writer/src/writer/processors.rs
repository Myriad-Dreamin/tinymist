//! Node processor implementations for CommonMark writer.
//!
//! This module contains the node processing strategies for handling different types of nodes.

use crate::ast::Node;
use crate::error::{WriteError, WriteResult};

// CommonMarkWriter is imported via super to avoid circular dependencies
use super::cmark::CommonMarkWriter;

/// Private trait for node processing strategy
pub(crate) trait NodeProcessor {
    /// Process a node and write its content
    fn process(&self, writer: &mut CommonMarkWriter, node: &Node) -> WriteResult<()>;
}

/// Strategy for processing block nodes
pub(crate) struct BlockNodeProcessor;

/// Strategy for processing inline nodes
pub(crate) struct InlineNodeProcessor;

/// Strategy for processing custom nodes
pub(crate) struct CustomNodeProcessor;

impl NodeProcessor for BlockNodeProcessor {
    fn process(&self, writer: &mut CommonMarkWriter, node: &Node) -> WriteResult<()> {
        match node {
            Node::Document(children) => writer.write_document(children),
            Node::Heading {
                level,
                content,
                heading_type,
            } => writer.write_heading(*level, content, heading_type),
            Node::Paragraph(content) => writer.write_paragraph(content),
            Node::BlockQuote(content) => writer.write_blockquote(content),
            Node::CodeBlock {
                language,
                content,
                block_type,
            } => writer.write_code_block(language, content, block_type),
            Node::UnorderedList(items) => writer.write_unordered_list(items),
            Node::OrderedList { start, items } => writer.write_ordered_list(*start, items),
            Node::ThematicBreak => writer.write_thematic_break(),

            #[cfg(feature = "gfm")]
            Node::Table {
                headers,
                alignments,
                rows,
            } => writer.write_table_with_alignment(headers, alignments, rows),

            #[cfg(not(feature = "gfm"))]
            Node::Table { headers, rows, .. } => writer.write_table(headers, rows),

            Node::HtmlBlock(content) => writer.write_html_block(content),
            Node::LinkReferenceDefinition {
                label,
                destination,
                title,
            } => writer.write_link_reference_definition(label, destination, title),
            Node::Custom(custom_node) if custom_node.is_block() => {
                writer.write_custom_node(custom_node)
            }
            _ => Err(WriteError::UnsupportedNodeType),
        }?;

        writer.ensure_trailing_newline()?;

        Ok(())
    }
}

impl NodeProcessor for InlineNodeProcessor {
    fn process(&self, writer: &mut CommonMarkWriter, node: &Node) -> WriteResult<()> {
        // Check for newlines in inline nodes in strict mode
        if writer.is_strict_mode() && !matches!(node, Node::SoftBreak | Node::HardBreak) {
            let context = writer.get_context_for_node(node);
            writer.check_no_newline(node, &context)?;
        }

        match node {
            Node::Text(content) => writer.write_text_content(content),
            Node::Emphasis(content) => writer.write_emphasis(content),
            Node::Strong(content) => writer.write_strong(content),

            #[cfg(feature = "gfm")]
            Node::Strikethrough(content) => writer.write_strikethrough(content),

            Node::InlineCode(content) => writer.write_code_content(content),
            Node::Link {
                url,
                title,
                content,
            } => writer.write_link(url, title, content),
            Node::Image { url, title, alt } => writer.write_image(url, title, alt),
            Node::Autolink { url, is_email } => writer.write_autolink(url, *is_email),

            #[cfg(feature = "gfm")]
            Node::ExtendedAutolink(url) => writer.write_extended_autolink(url),

            Node::ReferenceLink { label, content } => writer.write_reference_link(label, content),
            Node::HtmlElement(element) => writer.write_html_element(element),
            Node::SoftBreak => writer.write_soft_break(),
            Node::HardBreak => writer.write_hard_break(),
            Node::Custom(custom_node) if !custom_node.is_block() => {
                writer.write_custom_node(custom_node)
            }
            _ => Err(WriteError::UnsupportedNodeType),
        }
    }
}

impl NodeProcessor for CustomNodeProcessor {
    fn process(&self, writer: &mut CommonMarkWriter, node: &Node) -> WriteResult<()> {
        match node {
            Node::Custom(custom_node) => {
                writer.write_custom_node(custom_node)?;

                if custom_node.is_block() {
                    writer.ensure_trailing_newline()?;
                }

                Ok(())
            }
            _ => Err(WriteError::UnsupportedNodeType),
        }
    }
}
