//! GitHub Flavored Markdown (GFM) extensions
//!
//! This module provides support for GitHub Flavored Markdown extensions
//! including tables with alignment, strikethrough text, task lists,
//! extended autolinks, and HTML tag filtering.
//!
//! All GFM features are only available when the `gfm` feature is enabled.

pub use crate::ast::{TableAlignment, TaskListStatus};
pub use crate::options::WriterOptionsBuilder;

pub mod formatting;
pub mod tables;
pub mod tasks;

/// Creates writer options with all GFM extensions enabled
///
/// # Returns
///
/// WriterOptions with all GFM features enabled
pub fn gfm_options() -> crate::options::WriterOptions {
    WriterOptionsBuilder::new().enable_gfm().build()
}
