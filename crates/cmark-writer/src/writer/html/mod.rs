//! Provides HTML rendering capabilities, including the `HtmlWriter`,
//! rendering options, and error types specific to HTML generation.

/// HTML error types used during HTML writing.
pub mod error;
/// Options for configuring HTML rendering behavior.
pub mod options;
pub mod utils;
/// The core `HtmlWriter` and its implementation for generating HTML.
pub mod writer;

pub use self::error::{HtmlWriteError, HtmlWriteResult};
pub use self::options::HtmlWriterOptions;
pub use self::writer::HtmlWriter;
