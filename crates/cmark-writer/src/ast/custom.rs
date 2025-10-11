//! Custom node definitions for the CommonMark AST.

use crate::error::{WriteError, WriteResult};
use crate::writer::{BlockWriterProxy, HtmlWriteResult, HtmlWriter, InlineWriterProxy};
use std::any::Any;

/// Trait for implementing custom node behavior for the CommonMark AST.
///
/// This trait defines methods that all custom node types must implement.
/// Users can implement dedicated block or inline rendering methods for CommonMark output and
/// optionally override the `html_write` method for HTML output.
///
/// The recommended way to implement this trait is through the `custom_node` macro,
/// which provides a default implementation of most methods and requires users to
/// implement only the node-specific logic.
///
/// # Example
///
/// ```rust
/// use ecow::EcoString;
/// use cmark_writer_macros::custom_node;
/// use cmark_writer::error::WriteResult;
/// use cmark_writer::writer::{HtmlWriteResult, HtmlWriter, InlineWriterProxy};
///
/// // Define a custom node with support for both CommonMark and HTML output
/// #[derive(Debug, Clone, PartialEq)]
/// #[custom_node(block=false, html_impl=true)]
/// struct HighlightNode {
///     content: EcoString,
///     color: EcoString,
/// }
///
/// impl HighlightNode {
///     // Required for CommonMark output
///     fn write_custom(&self, writer: &mut InlineWriterProxy) -> WriteResult<()> {
///         writer.write_str("<span style=\"background-color: ")?;
///         writer.write_str(&self.color)?;
///         writer.write_str("\">")?;
///         writer.write_str(&self.content)?;
///         writer.write_str("</span>")?;
///         Ok(())
///     }
///
///     // Optional HTML-specific implementation
///     fn write_html_custom(&self, writer: &mut HtmlWriter) -> HtmlWriteResult<()> {
///         writer.start_tag("span")?;
///         writer.attribute("style", &format!("background-color: {}", self.color))?;
///         writer.finish_tag()?;
///         writer.text(&self.content)?;
///         writer.end_tag("span")?;
///         Ok(())
///     }
/// }
/// ```
pub trait CustomNode: std::fmt::Debug + Send + Sync {
    /// Write the custom node as a block element using the restricted block writer proxy.
    ///
    /// Block custom nodes should implement this method to emit valid block-level content.
    fn write_block(&self, writer: &mut BlockWriterProxy) -> WriteResult<()> {
        let _ = writer;
        Err(WriteError::UnsupportedNodeType)
    }

    /// Write the custom node as an inline element using the restricted inline writer proxy.
    ///
    /// Inline custom nodes should implement this method to emit valid inline content.
    fn write_inline(&self, writer: &mut InlineWriterProxy) -> WriteResult<()> {
        let _ = writer;
        Err(WriteError::UnsupportedNodeType)
    }

    /// Writes the HTML representation of the custom node to the provided HTML writer.
    ///
    /// By default, this writes an HTML comment indicating that HTML rendering is not implemented
    /// for this custom node type. When using the `custom_node` macro with `html_impl=true`,
    /// this method delegates to the user-defined `write_html_custom` method.
    ///
    /// Users should either:
    /// 1. Override this method directly, or
    /// 2. Use the `custom_node` macro with `html_impl=true` and implement the `write_html_custom` method.
    fn html_write(&self, writer: &mut HtmlWriter) -> HtmlWriteResult<()> {
        writer.raw_html(&format!(
            "<!-- HTML rendering not implemented for Custom Node: {} -->",
            self.type_name()
        ))?;
        Ok(())
    }

    /// Clone the custom node
    fn clone_box(&self) -> Box<dyn CustomNode>;

    /// Check if two custom nodes are equal
    fn eq_box(&self, other: &dyn CustomNode) -> bool;

    /// Whether the custom node is a block element
    fn is_block(&self) -> bool;

    /// Convert to Any for type casting
    fn as_any(&self) -> &dyn Any;

    /// Convert to mutable Any for type casting
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Get the type name of the custom node for pattern matching
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

// NOTE: CustomNodeWriter trait is deprecated and will be removed in a future version.
// Custom nodes should now directly use the provided writer proxies instead.
/*
/// Trait for custom node writer implementation
pub trait CustomNodeWriter {
    /// Write a string to the output
    fn write_str(&mut self, s: &str) -> std::fmt::Result;

    /// Write a character to the output
    fn write_char(&mut self, c: char) -> std::fmt::Result;
}
*/

// Implement Clone for Box<dyn CustomNode>
impl Clone for Box<dyn CustomNode> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

// Implement PartialEq for Box<dyn CustomNode>
impl PartialEq for Box<dyn CustomNode> {
    fn eq(&self, other: &Self) -> bool {
        self.eq_box(&**other)
    }
}
