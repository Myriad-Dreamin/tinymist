//! Project Model for tinymist

#![allow(missing_docs)]

mod args;
mod compiler;
pub mod font;
mod lock;
mod model;
mod watch;
pub mod world;
pub use args::*;
pub use compiler::*;
pub use lock::*;
pub use model::*;
pub use watch::*;
pub use world::*;

/// LsP interrupt.
pub type LspInterrupt = Interrupt<LspCompilerFeat>;
