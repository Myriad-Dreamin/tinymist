//! Analyzing the syntax of a source file.
//!
//! This module must hide all **AST details** from the rest of the codebase.

mod dependency;
pub(crate) mod docs;
mod expr;
pub(crate) mod index;
pub(crate) mod lexical_hierarchy;
pub(crate) mod module;

pub(crate) use dependency::*;
pub(crate) use expr::ExprRoute;
pub use index::*;
pub use lexical_hierarchy::*;
pub use module::*;
pub use tinymist_analysis::syntax::*;
