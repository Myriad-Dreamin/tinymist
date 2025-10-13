//! CommonMark writer implementation.
//!
//! This module provides the `CommonMarkWriter` along with supporting
//! helpers split across multiple submodules to keep individual files
//! focused and maintainable.

mod core;
mod format;
mod html_fallback;
mod nodes;
mod utils;

#[cfg(test)]
mod tests;

pub use core::CommonMarkWriter;
