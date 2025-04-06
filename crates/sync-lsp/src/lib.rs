//! A synchronous language server implementation.

#[cfg(feature = "dap")]
pub mod dap;
#[cfg(feature = "lsp")]
pub mod lsp;

mod error;
pub use error::*;

mod msg;
pub use msg::*;

#[cfg(feature = "server")]
pub use server::*;
#[cfg(feature = "server")]
pub mod req_queue;
#[cfg(feature = "server")]
mod server;
#[cfg(feature = "server")]
pub mod transport;

use std::any::Any;

/// The common error type for language servers.
pub use crate::msg::ResponseError;
/// The common result type for language servers.
pub type LspResult<T> = Result<T, ResponseError>;
/// The common event type for language servers.
pub type Event = Box<dyn Any + Send>;
