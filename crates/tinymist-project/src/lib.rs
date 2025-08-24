//! Project Model for tinymist

mod args;
mod compiler;
mod entry;
mod lock;
mod model;

#[cfg(feature = "lsp")]
mod lsp;
#[cfg(feature = "system")]
mod watch;

pub mod world;

pub use args::*;
pub use compiler::*;
pub use entry::*;
pub use lock::*;
pub use model::*;
pub use world::*;

#[cfg(feature = "lsp")]
pub use lsp::*;
#[cfg(feature = "system")]
pub use watch::*;

pub use tinymist_world::{CompileSignal, CompileSnapshot, ProjectInsId};

/// The default project route priority assigned to user actions.
pub const PROJECT_ROUTE_USER_ACTION_PRIORITY: u32 = 256;
