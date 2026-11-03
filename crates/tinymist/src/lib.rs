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

/// Placeholder value emitted by `vergen`'s idempotent mode when the
/// corresponding instruction fails to collect real data.
const VERGEN_IDEMPOTENT_OUTPUT: &str = "VERGEN_IDEMPOTENT_OUTPUT";

/// Returns the input value unless it is empty or a `vergen` idempotent
/// placeholder, in which case the package version is returned instead.
fn git_describe_or_version(value: &str) -> &str {
    if value.is_empty() || value == VERGEN_IDEMPOTENT_OUTPUT {
        return env!("CARGO_PKG_VERSION");
    }
    value
}

/// Returns the input value unless it is empty or a `vergen` idempotent
/// placeholder, in which case `None` is returned instead (same pattern as
/// `VERGEN_GIT_SHA` above).
fn value_or_none(value: &str) -> &str {
    if value.is_empty() || value == VERGEN_IDEMPOTENT_OUTPUT {
        return "None";
    }
    value
}

/// The long version description of the library
pub static LONG_VERSION: LazyLock<String> = LazyLock::new(|| {
    format!(
        "v{}
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
        git_describe_or_version(env!("VERGEN_GIT_DESCRIBE")),
        option_env!("VERGEN_GIT_SHA").unwrap_or("None"),
        value_or_none(option_env!("VERGEN_GIT_COMMIT_TIMESTAMP").unwrap_or_default()),
        value_or_none(option_env!("VERGEN_GIT_BRANCH").unwrap_or_default()),
        env!("VERGEN_CARGO_TARGET_TRIPLE"),
        env!("TYPST_VERSION"),
        env!("TYPST_SOURCE"),
    )
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_long_version_first_line_has_version() {
        let first_line = LONG_VERSION.lines().next().unwrap();
        let expected = format!("v{}", env!("CARGO_PKG_VERSION"));
        assert_eq!(first_line, expected);
        assert!(first_line.starts_with('v'));
        assert!(first_line.len() > 1);
    }

    #[test]
    fn test_git_describe_or_version_falls_back() {
        assert_eq!(git_describe_or_version(""), env!("CARGO_PKG_VERSION"));
        assert_eq!(
            git_describe_or_version("VERGEN_IDEMPOTENT_OUTPUT"),
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(git_describe_or_version("v0.15.8"), "v0.15.8");
    }

    #[test]
    fn test_value_or_none_falls_back() {
        assert_eq!(value_or_none(""), "None");
        assert_eq!(value_or_none("VERGEN_IDEMPOTENT_OUTPUT"), "None");
        assert_eq!(value_or_none("main"), "main");
    }
}
