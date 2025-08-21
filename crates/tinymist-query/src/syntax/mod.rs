//! Analyzing the syntax of a source file.
//!
//! This module must hide all **AST details** from the rest of the codebase.

pub(crate) mod docs;
pub(crate) mod expr;
pub(crate) mod index;
pub(crate) mod lexical_hierarchy;
pub(crate) mod module;

pub use expr::*;
pub use index::*;
pub use lexical_hierarchy::*;
pub use module::*;
pub use tinymist_analysis::syntax::*;
