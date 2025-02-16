//! Project Model for tinymist

mod args;
mod compiler;
mod entry;
mod model;

#[cfg(feature = "lsp")]
pub mod font;
#[cfg(feature = "lsp")]
mod lock;
#[cfg(feature = "lsp")]
mod lsp;
#[cfg(feature = "system")]
mod watch;
#[cfg(feature = "system")]
pub mod world;

pub use args::*;
pub use compiler::*;
pub use entry::*;
pub use model::*;

#[cfg(feature = "lsp")]
pub use lock::*;
#[cfg(feature = "lsp")]
pub use lsp::*;
#[cfg(feature = "system")]
pub use watch::*;
#[cfg(feature = "system")]
pub use world::*;

pub use tinymist_world::{CompileSnapshot, ExportSignal, ProjectInsId};

/// The default project route priority assigned to user actions.
pub const PROJECT_ROUTE_USER_ACTION_PRIORITY: u32 = 256;
