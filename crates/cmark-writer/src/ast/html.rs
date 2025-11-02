//! HTML element definitions and utilities for CommonMark AST.
//!
//! This module contains definitions for HTML elements and attributes in the AST,
//! along with utilities for safely handling HTML content.

use super::Node;
use ecow::EcoString;

/// HTML attribute
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlAttribute {
    /// Attribute name
    pub name: EcoString,
    /// Attribute value
    pub value: EcoString,
}

/// HTML element
#[derive(Debug, Clone, PartialEq)]
pub struct HtmlElement {
    /// HTML tag name
    pub tag: EcoString,
    /// HTML attributes
    pub attributes: Vec<HtmlAttribute>,
    /// Child nodes
    pub children: Vec<Node>,
    /// Whether this is a self-closing element
    pub self_closing: bool,
}

impl HtmlElement {
    /// Create a new HTML element
    pub fn new(tag: &str) -> Self {
        Self {
            tag: tag.into(),
            attributes: Vec::new(),
            children: Vec::new(),
            self_closing: false,
        }
    }

    /// Add an attribute to the HTML element
    pub fn with_attribute(mut self, name: &str, value: &str) -> Self {
        self.attributes.push(HtmlAttribute {
            name: name.into(),
            value: value.into(),
        });
        self
    }

    /// Add multiple attributes to the HTML element
    pub fn with_attributes(mut self, attrs: Vec<(&str, &str)>) -> Self {
        for (name, value) in attrs {
            self.attributes.push(HtmlAttribute {
                name: name.into(),
                value: value.into(),
            });
        }
        self
    }

    /// Add child nodes to the HTML element
    pub fn with_children(mut self, children: Vec<Node>) -> Self {
        self.children = children;
        self
    }

    /// Set whether the element is self-closing
    pub fn self_closing(mut self, is_self_closing: bool) -> Self {
        self.self_closing = is_self_closing;
        self
    }

    /// Check if this element's tag matches any in the provided list (case-insensitive)
    pub fn tag_matches_any(&self, tags: &[EcoString]) -> bool {
        tags.iter().any(|tag| tag.eq_ignore_ascii_case(&self.tag))
    }
}
