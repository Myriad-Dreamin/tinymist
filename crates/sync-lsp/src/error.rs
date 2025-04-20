#![allow(unused)]

use std::fmt;

#[cfg(feature = "lsp")]
use crate::lsp::{Notification, Request};

/// A protocol error happened during communication through LSP or DAP.
#[derive(Debug, Clone, PartialEq)]
pub struct ProtocolError(String, bool);

impl ProtocolError {
    /// Creates a protocol error with a message.
    pub(crate) fn new(msg: impl Into<String>) -> Self {
        ProtocolError(msg.into(), false)
    }

    /// Creates a protocol error caused by disconnection.
    pub(crate) fn disconnected() -> ProtocolError {
        ProtocolError("disconnected channel".into(), true)
    }

    /// Whether this error occurred due to a disconnected channel.
    pub fn channel_is_disconnected(&self) -> bool {
        self.1
    }
}

impl std::error::Error for ProtocolError {}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Failure of decoding happened during communication
/// through LSP or DAP.
#[derive(Debug)]
pub enum ExtractError<T> {
    /// The extracted message was of a different method than expected.
    MethodMismatch(T),
    /// Failed to deserialize the message.
    JsonError {
        /// The method is being decoded
        method: String,
        /// The underlying error
        error: serde_json::Error,
    },
}

#[cfg(feature = "lsp")]
impl std::error::Error for ExtractError<Request> {}
#[cfg(feature = "lsp")]
impl fmt::Display for ExtractError<Request> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtractError::MethodMismatch(req) => {
                write!(f, "Method mismatch for request '{}'", req.method)
            }
            ExtractError::JsonError { method, error } => {
                write!(f, "Invalid request\nMethod: {method}\n error: {error}",)
            }
        }
    }
}

#[cfg(feature = "lsp")]
impl std::error::Error for ExtractError<Notification> {}
#[cfg(feature = "lsp")]
impl fmt::Display for ExtractError<Notification> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtractError::MethodMismatch(req) => {
                write!(f, "Method mismatch for notification '{}'", req.method)
            }
            ExtractError::JsonError { method, error } => {
                write!(f, "Invalid notification\nMethod: {method}\n error: {error}")
            }
        }
    }
}
