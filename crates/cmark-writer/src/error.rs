//! Error handling for CommonMark writer.
//!
//! This module provides error types and implementations for handling errors
//! that can occur during CommonMark writing.

use ecow::EcoString;

use crate::writer::html::error::HtmlWriteError as CoreHtmlWriteError;
use std::error::Error;
use std::fmt::{self, Display};
use std::io;

/// Errors that can occur during CommonMark writing.
#[derive(Debug)]
pub enum WriteError {
    /// An invalid heading level was encountered (must be 1-6).
    InvalidHeadingLevel(u8),
    /// A newline character was found in an inline element where it's not allowed (e.g., in strict mode or specific contexts like table cells, link text, image alt text).
    NewlineInInlineElement(EcoString),
    /// An underlying formatting error occurred.
    FmtError(EcoString),
    /// An underlying I/O error occurred.
    IoError(io::Error),
    /// An unsupported node type was encountered.
    UnsupportedNodeType,
    /// Invalid structure in a node (e.g., mismatched table columns)
    InvalidStructure(EcoString),
    /// An invalid HTML tag was found (contains unsafe characters)
    InvalidHtmlTag(EcoString),
    /// An invalid HTML attribute was found (contains unsafe characters)
    InvalidHtmlAttribute(EcoString),
    /// An error occurred during dedicated HTML rendering.
    HtmlRenderingError(CoreHtmlWriteError),
    /// An error occurred during HTML fallback rendering for tables with block elements.
    HtmlFallbackError(EcoString),
    /// A custom error with a message and optional error code.
    Custom {
        /// Custom error message
        message: EcoString,
        /// Optional error code for programmatic identification
        code: Option<EcoString>,
    },
}

impl Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WriteError::InvalidHeadingLevel(level) => write!(
                f,
                "Invalid heading level: {}. Level must be between 1 and 6.",
                level
            ),
            WriteError::NewlineInInlineElement(context) => write!(
                f,
                "Newline character found within an inline element ({}) which is not allowed in strict mode or this context.",
                context
            ),
            WriteError::FmtError(msg) => write!(f, "Formatting error: {}", msg),
            WriteError::IoError(err) => write!(f, "I/O error: {}", err),
            WriteError::UnsupportedNodeType => {
                write!(f, "Unsupported node type encountered during writing.")
            },
            WriteError::InvalidStructure(msg) => {
                write!(f, "Invalid structure: {}", msg)
            },
            WriteError::InvalidHtmlTag(tag) => {
                write!(f, "Invalid HTML tag name: '{}'. Tag names should only contain alphanumeric characters, underscores, colons, or hyphens.", tag)
            },
            WriteError::InvalidHtmlAttribute(attr) => {
                write!(f, "Invalid HTML attribute name: '{}'. Attribute names should only contain alphanumeric characters, underscores, colons, dots, or hyphens.", attr)
            },
            WriteError::HtmlRenderingError(html_err) => {
                write!(f, "Error during HTML rendering phase: {}", html_err)
            },
            WriteError::HtmlFallbackError(msg) => {
                write!(f, "Error during HTML fallback rendering: {}", msg)
            },
            WriteError::Custom { message, code } => {
                if let Some(code) = code {
                    write!(f, "Custom error [{}]: {}", code, message)
                } else {
                    write!(f, "Custom error: {}", message)
                }
            }
        }
    }
}

impl Error for WriteError {}

// Allow converting fmt::Error into WriteError for convenience when using `?`
impl From<fmt::Error> for WriteError {
    fn from(err: fmt::Error) -> Self {
        WriteError::FmtError(err.to_string().into())
    }
}

// Allow converting io::Error into WriteError
impl From<io::Error> for WriteError {
    fn from(err: io::Error) -> Self {
        WriteError::IoError(err)
    }
}

// Allow converting CoreHtmlWriteError into WriteError
impl From<CoreHtmlWriteError> for WriteError {
    fn from(err: CoreHtmlWriteError) -> Self {
        match err {
            CoreHtmlWriteError::InvalidHtmlTag(tag) => WriteError::InvalidHtmlTag(tag.into()),
            CoreHtmlWriteError::InvalidHtmlAttribute(attr) => {
                WriteError::InvalidHtmlAttribute(attr.into())
            }
            other_html_err => WriteError::HtmlRenderingError(other_html_err),
        }
    }
}

/// Result type alias for writer operations.
pub type WriteResult<T> = Result<T, WriteError>;

/// Convenience methods for creating custom errors
impl WriteError {
    /// Create a new custom error with a message
    pub fn custom<S: Into<EcoString>>(message: S) -> Self {
        WriteError::Custom {
            message: message.into(),
            code: None,
        }
    }

    /// Create a new custom error with a message and error code
    pub fn custom_with_code<S1: Into<EcoString>, S2: Into<EcoString>>(
        message: S1,
        code: S2,
    ) -> Self {
        WriteError::Custom {
            message: message.into(),
            code: Some(code.into()),
        }
    }
}

/// Trait to define custom error factories for WriteError
///
/// This trait allows extending WriteError with custom error constructors
/// while allowing both library and user code to define their own error types.
pub trait CustomErrorFactory {
    /// Create an error from this factory
    fn create_error(&self) -> WriteError;
}

/// Struct to create structure errors with formatted messages
pub struct StructureError {
    /// Format string for the error message
    format: EcoString,
    /// Arguments for formatting
    args: Vec<EcoString>,
}

impl StructureError {
    /// Create a new structure error with a format string and arguments
    pub fn new<S: Into<EcoString>>(format: S) -> Self {
        Self {
            format: format.into(),
            args: Vec::new(),
        }
    }

    /// Add an argument to the format string
    pub fn arg<S: Into<EcoString>>(mut self, arg: S) -> Self {
        self.args.push(arg.into());
        self
    }
}

impl CustomErrorFactory for StructureError {
    fn create_error(&self) -> WriteError {
        let message = match self.args.len() {
            0 => self.format.clone(),
            1 => self.format.replace("{}", &self.args[0]),
            _ => {
                let mut result = self.format.to_string();
                for arg in &self.args {
                    if let Some(pos) = result.find("{}") {
                        result.replace_range(pos..pos + 2, arg);
                    }
                }
                EcoString::from(result)
            }
        };

        WriteError::InvalidStructure(message)
    }
}

/// Struct to create custom errors with codes
pub struct CodedError {
    /// The error message
    message: EcoString,
    /// The error code
    code: EcoString,
}

impl CodedError {
    /// Create a new custom error with message and code
    pub fn new<S1: Into<EcoString>, S2: Into<EcoString>>(message: S1, code: S2) -> Self {
        Self {
            message: message.into(),
            code: code.into(),
        }
    }
}

impl CustomErrorFactory for CodedError {
    fn create_error(&self) -> WriteError {
        WriteError::custom_with_code(&self.message, &self.code)
    }
}

/// Extensions for Result<T, WriteError> to work with custom error factories
pub trait WriteResultExt<T> {
    /// Convert a custom error factory into an Err result
    fn custom_error<F: CustomErrorFactory>(factory: F) -> Result<T, WriteError>;
}

impl<T> WriteResultExt<T> for Result<T, WriteError> {
    fn custom_error<F: CustomErrorFactory>(factory: F) -> Result<T, WriteError> {
        Err(factory.create_error())
    }
}
