//! Abstract Syntax Tree for CommonMark document structure.
//!
//! This module defines various node types for representing CommonMark documents,
//! including headings, paragraphs, lists, code blocks, etc.

mod custom;
mod html;
mod node;
pub mod tables;

pub use self::custom::CustomNode;
pub use self::html::{HtmlAttribute, HtmlElement};
pub use self::node::{CodeBlockType, HeadingType, ListItem, Node};

// Re-export GFM specific types when the GFM feature is enabled
#[cfg(feature = "gfm")]
pub use self::node::{TableAlignment, TaskListStatus};
