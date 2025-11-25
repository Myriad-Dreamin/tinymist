//! # tinymist
//!
//! This crate provides a CLI that starts services for [Typst](https://typst.app/). It provides:
//! + `tinymist lsp`: A language server following the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/).
//! + `tinymist preview`: A preview server for Typst.
//!
//! ## Usage
//!
//! See [Features: Command Line Interface](https://myriad-dreamin.github.io/tinymist/feature/cli.html).
//!
//! ## Documentation
//!
//! See [Crate Docs](https://myriad-dreamin.github.io/tinymist/rs/tinymist/index.html).
//!
//! Also see [Developer Guide: Tinymist LSP](https://myriad-dreamin.github.io/tinymist/module/lsp.html).
//!
//! ## Contributing
//!
//! See [CONTRIBUTING.md](https://github.com/Myriad-Dreamin/tinymist/blob/main/CONTRIBUTING.md).

pub use config::*;
pub use log_::*;
pub use lsp::init::*;
pub use server::*;
pub use sync_ls::LspClient;
pub use tinymist_project::world;
pub use tinymist_query as query;
pub use world::{CompileFontArgs, CompileOnceArgs, CompilePackageArgs};

#[cfg(feature = "export")]
pub use task::export2 as export;
#[cfg(feature = "export")]
pub use task::ExportTask;
#[cfg(feature = "trace")]
pub use task::UserActionTask;

#[cfg(feature = "dap")]
pub use dap::RegularInit as DapRegularInit;
#[cfg(feature = "dap")]
pub use dap::SuperInit as DapSuperInit;

pub mod project;
pub mod tool;

#[cfg(feature = "web")]
pub mod web;

mod actor;
mod cmd;
mod config;
mod input;
#[path = "log.rs"]
mod log_;
mod lsp;
mod resource;
mod server;
mod stats;
mod task;
mod utils;

#[cfg(feature = "dap")]
mod dap;
#[cfg(feature = "lock")]
mod route;

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
