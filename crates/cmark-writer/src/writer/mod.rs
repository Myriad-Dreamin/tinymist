//! CommonMark writer implementation.
//!
//! This module provides functionality to convert AST nodes to various formats.

mod cmark;
pub mod runtime;

pub use self::cmark::CommonMarkWriter;

/// HTML specific modules are now grouped under writer::html
pub mod html;
pub use self::html::{HtmlWriteError, HtmlWriteResult, HtmlWriter, HtmlWriterOptions};

pub use self::runtime::diagnostics::{
    Diagnostic, DiagnosticSeverity, DiagnosticSink, NullSink, SharedVecSink,
};
pub use self::runtime::proxy::{BlockWriterProxy, InlineWriterProxy};
pub use self::runtime::visitor::{walk_node, NodeHandler};
