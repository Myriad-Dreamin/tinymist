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
mod init;
pub(crate) mod input;
pub(crate) mod lsp;
pub(crate) mod lsp_query;
pub mod project;
mod resource;
pub(crate) mod route;
mod server;
mod stats;
mod task;
pub mod tool;
mod utils;

pub use init::*;
pub use server::*;
pub use sync_lsp::LspClient;
pub use task::export2 as export;
pub use task::UserActionTask;
pub use tinymist_project::world;
pub use tinymist_query as query;
pub use world::{CompileFontArgs, CompileOnceArgs, CompilePackageArgs};

use lsp_query::QueryFuture;
use lsp_server::ResponseError;
use serde_json::from_value;
use sync_lsp::*;
use utils::*;
use world::*;
