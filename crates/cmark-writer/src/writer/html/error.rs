use crate::error::WriteError;
use std::fmt::{self, Display};
use std::io;

/// Errors that can occur during HTML writing from AST nodes.
#[derive(Debug)]
pub enum HtmlWriteError {
    /// An underlying I/O error occurred.
    Io(io::Error),
    /// A node type is not supported for HTML conversion (or not yet implemented).
    UnsupportedNodeType(String),
    /// Invalid structure or content encountered during HTML conversion.
    InvalidStructure(String),
    /// An invalid HTML tag name was encountered.
    InvalidHtmlTag(String),
    /// An invalid HTML attribute name was encountered.
    InvalidHtmlAttribute(String),
    /// An error occurred while writing a custom node.
    CustomNodeError(String),
    // Add more specific HTML-related errors as needed
}

impl Display for HtmlWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HtmlWriteError::Io(err) => write!(f, "HTML I/O error: {err}"),
            HtmlWriteError::UnsupportedNodeType(node_type) => {
                write!(
                    f,
                    "HTML conversion not supported for node type: {node_type}"
                )
            }
            HtmlWriteError::InvalidStructure(msg) => {
                write!(f, "Invalid structure for HTML conversion: {msg}")
            }
            HtmlWriteError::InvalidHtmlTag(tag_name) => {
                write!(f, "Invalid HTML tag name: {tag_name}")
            }
            HtmlWriteError::InvalidHtmlAttribute(attr_name) => {
                write!(f, "Invalid HTML attribute name: {attr_name}")
            }
            HtmlWriteError::CustomNodeError(msg) => {
                write!(f, "Error writing custom node: {msg}")
            }
        }
    }
}

impl std::error::Error for HtmlWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            HtmlWriteError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl HtmlWriteError {
    /// Converts an `HtmlWriteError` into a `WriteError`.
    pub fn into_write_error(self) -> WriteError {
        match self {
            HtmlWriteError::Io(err) => WriteError::IoError(err),
            HtmlWriteError::UnsupportedNodeType(msg) => WriteError::Custom {
                message: format!("HTML writer error: {msg}").into(),
                code: None,
            },
            HtmlWriteError::InvalidStructure(msg) => WriteError::InvalidStructure(msg.into()),
            HtmlWriteError::InvalidHtmlTag(tag) => WriteError::InvalidHtmlTag(tag.into()),
            HtmlWriteError::InvalidHtmlAttribute(attr) => {
                WriteError::InvalidHtmlAttribute(attr.into())
            }
            HtmlWriteError::CustomNodeError(msg) => WriteError::Custom {
                message: format!("Custom node error: {msg}").into(),
                code: None,
            },
        }
    }
}

// Allow converting io::Error into HtmlWriteError for convenience when using `?`
impl From<io::Error> for HtmlWriteError {
    fn from(err: io::Error) -> Self {
        HtmlWriteError::Io(err)
    }
}

/// Result type alias for HTML writer operations from AST.
pub type HtmlWriteResult<T> = Result<T, HtmlWriteError>;
