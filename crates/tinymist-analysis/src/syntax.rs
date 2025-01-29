//! Analyzing the syntax of a source file.
//!
//! This module must hide all **AST details** from the rest of the codebase.

pub mod import;
pub use import::*;
pub mod comment;
pub use comment::*;
pub mod matcher;
pub use matcher::*;
