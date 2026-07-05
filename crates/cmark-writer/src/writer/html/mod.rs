/// The core `HtmlWriter` and its implementation for generating HTML.
pub mod core;
/// HTML error types used during HTML writing.
pub mod error;
pub(crate) mod nodes;
/// Options for configuring HTML rendering behavior.
pub mod options;
pub mod utils;

#[cfg(test)]
mod tests;

pub use self::core::HtmlWriter;
pub use self::error::{HtmlWriteError, HtmlWriteResult};
pub use self::options::HtmlWriterOptions;
