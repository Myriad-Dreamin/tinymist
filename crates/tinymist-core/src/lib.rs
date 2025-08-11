//! Tinymist Core Library

pub use config::*;
pub use dap::RegularInit as DapRegularInit;
pub use dap::SuperInit as DapSuperInit;
pub use lsp::init::*;
pub use server::*;
pub use sync_ls::LspClient;
pub use task::export2 as export;
pub use task::UserActionTask;
pub use tinymist_project::world;
pub use tinymist_query as query;
pub use world::{CompileFontArgs, CompileOnceArgs, CompilePackageArgs};

pub mod project;
pub mod tool;

pub(crate) mod config;
pub(crate) mod dap;
pub(crate) mod input;
pub(crate) mod lsp;
pub(crate) mod route;

mod actor;
mod cmd;
mod resource;
mod server;
mod stats;
mod task;
mod utils;

use std::sync::LazyLock;

use lsp::query::QueryFuture;
use serde_json::from_value;
use sync_ls::*;
use utils::*;
use world::*;

/// The long version description of the library
pub static LONG_VERSION: LazyLock<String> = LazyLock::new(|| {
    format!(
        "
Build Timestamp:     {}
Build Git Describe:  {}
Commit SHA:          {}
Commit Date:         {}
Commit Branch:       {}
Cargo Target Triple: {}
Typst Version:       {}
Typst Source:        {}
",
        env!("VERGEN_BUILD_TIMESTAMP"),
        env!("VERGEN_GIT_DESCRIBE"),
        option_env!("VERGEN_GIT_SHA").unwrap_or("None"),
        option_env!("VERGEN_GIT_COMMIT_TIMESTAMP").unwrap_or("None"),
        option_env!("VERGEN_GIT_BRANCH").unwrap_or("None"),
        env!("VERGEN_CARGO_TARGET_TRIPLE"),
        env!("TYPST_VERSION"),
        env!("TYPST_SOURCE"),
    )
});

#[cfg(feature = "web")]
pub mod web;
