//! A linter for Typst.
//!
//! Enable `lint-v2` to switch to the CFG-based lint implementation.

#[cfg(not(feature = "lint-v2"))]
mod v1;
#[cfg(feature = "lint-v2")]
mod v2;

#[cfg(not(feature = "lint-v2"))]
pub use v1::*;
#[cfg(feature = "lint-v2")]
pub use v2::*;
