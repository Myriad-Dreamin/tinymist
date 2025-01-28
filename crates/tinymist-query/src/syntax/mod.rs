//! Analyzing the syntax of a source file.
//!
//! This module must hide all **AST details** from the rest of the codebase.

// todo: remove this
#![allow(missing_docs)]

pub use tinymist_analysis::syntax::comment::*;
pub use tinymist_analysis::syntax::import::*;
pub use tinymist_analysis::syntax::matcher::*;
pub(crate) mod lexical_hierarchy;
pub use lexical_hierarchy::*;
pub(crate) mod module;
pub use module::*;
pub(crate) mod expr;
pub use expr::*;
pub(crate) mod docs;
pub use docs::*;
pub(crate) mod def;
pub use def::*;
pub(crate) mod repr;
use repr::*;
pub(crate) mod index;
pub use index::*;
