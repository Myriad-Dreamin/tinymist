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
    // Vergen emits an empty string when a git command fails, and
    // `VERGEN_IDEMPOTENT_OUTPUT` in idempotent builds (some release pipeline
    // builds). Fallback to the package version or `None` accordingly, so
    // `tinymist --version` always reports a readable version.
    let describe = match env!("VERGEN_GIT_DESCRIBE") {
        "" | "VERGEN_IDEMPOTENT_OUTPUT" => env!("CARGO_PKG_VERSION"),
        s => s,
    };
    let git_env = |v: Option<&'static str>| match v {
        None | Some("") | Some("VERGEN_IDEMPOTENT_OUTPUT") => "None",
        Some(s) => s,
    };
    format!(
        "{}
Build Timestamp:     {}
Build Git Describe:  {}
Commit SHA:          {}
Commit Date:         {}
Commit Branch:       {}
Cargo Target Triple: {}
Typst Version:       {}
Typst Source:        {}
",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_BUILD_TIMESTAMP"),
        describe,
        git_env(option_env!("VERGEN_GIT_SHA")),
        git_env(option_env!("VERGEN_GIT_COMMIT_TIMESTAMP")),
        git_env(option_env!("VERGEN_GIT_BRANCH")),
        env!("VERGEN_CARGO_TARGET_TRIPLE"),
        env!("TYPST_VERSION"),
        env!("TYPST_SOURCE"),
    )
});

#[cfg(test)]
mod version_tests {
    use super::LONG_VERSION;

    #[test]
    fn long_version_starts_with_package_version() {
        let version = &*LONG_VERSION;
        let first_line = version.lines().next().unwrap();
        assert!(
            first_line == env!("CARGO_PKG_VERSION"),
            "unexpected first line: {first_line}"
        );

        let describe = version
            .lines()
            .find(|l| l.starts_with("Build Git Describe:"))
            .expect("missing describe line")
            .rsplit_once(':')
            .unwrap()
            .1
            .trim();
        assert!(
            !describe.is_empty() && describe != "VERGEN_IDEMPOTENT_OUTPUT",
            "unfilled git describe: {describe}"
        );
    }
}
