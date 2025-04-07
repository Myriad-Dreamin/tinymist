//! Analyzing the syntax of a source file.
//!
//! This module must hide all **AST details** from the rest of the codebase.

#![allow(missing_docs)]

pub(crate) mod lexical_hierarchy;
pub use lexical_hierarchy::*;
pub(crate) mod module;
pub use module::*;
pub(crate) mod expr;
pub use expr::*;
pub(crate) mod docs;
pub use docs::*;
pub(crate) mod index;
pub use index::*;
pub use tinymist_analysis::syntax::*;
