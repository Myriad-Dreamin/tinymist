/// The core `HtmlWriter` and its implementation for generating HTML.
pub mod core;
/// HTML error types used during HTML writing.
pub mod error;
/// Options for configuring HTML rendering behavior.
pub mod options;
pub mod utils;

pub use self::core::HtmlWriter;
pub use self::error::{HtmlWriteError, HtmlWriteResult};
pub use self::options::HtmlWriterOptions;
