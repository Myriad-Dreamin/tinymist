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

mod actor;
mod cmd;
pub(crate) mod config;
pub(crate) mod input;
pub(crate) mod lsp;
pub mod project;
mod resource;
pub(crate) mod route;
mod server;
mod stats;
mod task;
pub mod tool;
mod utils;

pub use config::*;
pub use lsp::init::*;
pub use server::*;
pub use sync_ls::LspClient;
pub use task::export2 as export;
pub use task::UserActionTask;
pub use tinymist_project::world;
pub use tinymist_query as query;
pub use world::{CompileFontArgs, CompileOnceArgs, CompilePackageArgs};

use lsp::query::QueryFuture;
use serde_json::from_value;
use sync_ls::*;
use utils::*;
use world::*;
