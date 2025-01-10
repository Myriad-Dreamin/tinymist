//! Project Model for tinymist
//!
//! The [`ProjectCompiler`] implementation borrowed from typst.ts.
//!
//! Please check `tinymist::actor::typ_client` for architecture details.

#![allow(missing_docs)]

mod lock;
pub use lock::*;
mod model;
pub use model::*;
mod args;
pub use args::*;
mod compiler;
pub use compiler::*;
