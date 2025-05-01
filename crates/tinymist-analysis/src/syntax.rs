//! Analyzing the syntax of a source file.
//!
//! This module must hide all **AST details** from the rest of the codebase.

// todo: remove this
#![allow(missing_docs)]

pub mod import;
pub use import::*;
pub mod comment;
pub use comment::*;
pub mod matcher;
pub use matcher::*;

pub mod def;
pub use def::*;
pub(crate) mod repr;
use repr::*;
