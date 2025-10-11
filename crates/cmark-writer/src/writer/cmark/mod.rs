//! CommonMark writer implementation.
//!
//! This module provides the `CommonMarkWriter` along with supporting
//! helpers split across multiple submodules to keep individual files
//! focused and maintainable.

mod blocks;
mod core;
mod format;
mod html_fallback;
mod inline;
mod utils;

#[cfg(test)]
mod tests;

pub use core::CommonMarkWriter;
