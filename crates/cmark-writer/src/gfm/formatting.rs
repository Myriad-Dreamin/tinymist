//! GFM text formatting extensions
//!
//! This module provides utilities for GitHub Flavored Markdown
//! text formatting extensions such as strikethrough.

use crate::ast::Node;

/// Creates a strikethrough node
///
/// # Arguments
/// * `content` - The content to be struck through
///
/// # Returns
/// A strikethrough node containing the provided content
pub fn strikethrough(content: Vec<Node>) -> Node {
    Node::Strikethrough(content)
}

/// Creates a strikethrough node with plain text content
///
/// # Arguments
/// * `text` - The text to be struck through
///
/// # Returns
/// A strikethrough node containing the text
pub fn strike_text(text: &str) -> Node {
    Node::Strikethrough(vec![Node::Text(text.into())])
}

/// Creates a combined formatted node with strikethrough and emphasis
///
/// # Arguments
/// * `text` - The text to format
///
/// # Returns
/// A node with both strikethrough and emphasis formatting
pub fn strike_and_emphasize(text: &str) -> Node {
    Node::Strikethrough(vec![Node::Emphasis(vec![Node::Text(text.into())])])
}

/// Creates a combined formatted node with strikethrough and strong emphasis
///
/// # Arguments
/// * `text` - The text to format
///
/// # Returns
/// A node with both strikethrough and strong emphasis formatting
pub fn strike_and_strong(text: &str) -> Node {
    Node::Strikethrough(vec![Node::Strong(vec![Node::Text(text.into())])])
}
