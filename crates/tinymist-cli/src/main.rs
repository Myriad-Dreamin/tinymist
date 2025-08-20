#![doc = include_str!("../README.md")]

mod conn;
mod utils;
mod cmd {
    #[cfg(feature = "export")]
    pub mod compile;
    pub mod completion;
    pub mod cov;
    #[cfg(feature = "dap")]
    pub mod dap;
    pub mod generate_script;
    pub mod lsp;
    #[cfg(feature = "preview")]
    pub mod preview;
    pub mod query;
    pub mod test;
    pub mod trace_lsp;

    #[cfg(feature = "lock")]
    pub mod doc;
    #[cfg(feature = "lock")]
    pub mod task;
}

use std::sync::LazyLock;

use clap::Parser;
#[cfg(feature = "l10n")]
use tinymist_l10n::{load_translations, set_translations};
use tinymist_std::error::prelude::*;

use crate::cmd::*;
use crate::compile::CompileArgs;
use crate::conn::client_root;
use crate::utils::*;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// The runtimes used by the application.
pub struct Runtimes {
    /// The tokio runtime.
    pub tokio_runtime: tokio::runtime::Runtime,
}

impl Default for Runtimes {
    fn default() -> Self {
        let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        Self { tokio_runtime }
    }
}

static RUNTIMES: LazyLock<Runtimes> = LazyLock::new(Runtimes::default);

#[derive(Debug, Clone, clap::Parser)]
#[clap(name = "tinymist", author, version, about, long_version(tinymist::LONG_VERSION.as_str()))]
struct Args {
    /// Mode of the binary
    #[clap(subcommand)]
    pub cmd: Option<Commands>,
}

#[derive(Debug, Clone, clap::Subcommand)]
#[clap(rename_all = "kebab-case")]
enum Commands {
    /// Probes existence (Nop run)
    Probe,

    /// Runs language server
    Lsp(crate::lsp::LspArgs),
    /// Runs debug adapter
    #[cfg(feature = "dap")]
    Dap(crate::dap::DapArgs),
    /// Runs language server for tracing some typst program.
    #[clap(hide(true))]
    TraceLsp(crate::trace_lsp::TraceLspArgs),

    /// Runs language query
    #[clap(hide(true))] // still in development
    #[clap(subcommand)]
    Query(crate::query::QueryCommands),
    /// Runs preview server
    #[cfg(feature = "preview")]
    Preview(tinymist::tool::preview::PreviewCliArgs),
    /// Runs compile command like `typst-cli compile`
    Compile(CompileArgs),

    /// Generates completion script to stdout
    Completion(crate::completion::ShellCompletionArgs),
    /// Generates build script for compilation
    #[clap(hide(true))] // still in development
    GenerateScript(crate::generate_script::GenerateScriptArgs),

    /// Runs documents
    #[clap(hide(true))] // still in development
    #[clap(subcommand)]
    Doc(tinymist::project::DocCommands),
    /// Runs tasks
    #[cfg(feature = "lock")]
    #[clap(hide(true))] // still in development
    #[clap(subcommand)]
    Task(crate::task::TaskCommands),

    /// Execute a document and collect coverage
    #[clap(hide(true))] // still in development
    Cov(crate::cov::CovArgs),
    /// Test a document and gives summary
    Test(crate::test::TestArgs),
}

/// The main entry point.
fn main() -> Result<()> {
    // The root allocator for heap memory profiling.
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    // Parses command line arguments
    let cmd = Args::parse().cmd;
    let cmd = cmd.unwrap_or_else(|| Commands::Lsp(Default::default()));

    // Probes soon to avoid other initializations causing errors
    if matches!(cmd, Commands::Probe) {
        return Ok(());
    }

    // Loads translations
    #[cfg(feature = "l10n")]
    set_translations(load_translations(tinymist_assets::L10N_DATA)?);

    // Starts logging
    let _ = {
        use log::LevelFilter::*;

        let is_transient_cmd = matches!(cmd, Commands::Compile(..));
        let is_test_no_verbose = matches!(&cmd, Commands::Test(test) if !test.verbose);
        let base_no_info = is_transient_cmd || is_test_no_verbose;
        let base_level = if base_no_info { Warn } else { Info };
        let preview_level = if is_test_no_verbose { Warn } else { Debug };
        let diag_level = if is_test_no_verbose { Warn } else { Info };

        env_logger::builder()
            .filter_module("tinymist", base_level)
            .filter_module("tinymist_preview", preview_level)
            .filter_module("typlite", base_level)
            .filter_module("reflexo", base_level)
            .filter_module("sync_ls", base_level)
            .filter_module("reflexo_typst2vec::pass::span2vec", Error)
            .filter_module("reflexo_typst::diag::console", diag_level)
            .try_init()
    };

    match cmd {
        Commands::Probe => Ok(()),

        Commands::Lsp(args) => crate::lsp::lsp_main(args),
        #[cfg(feature = "dap")]
        Commands::Dap(args) => crate::dap::dap_main(args),
        Commands::TraceLsp(args) => crate::trace_lsp::trace_lsp_main(args),

        Commands::Query(cmds) => crate::query::query_main(cmds),
        #[cfg(feature = "preview")]
        Commands::Preview(args) => block_on(crate::preview::preview_main(args)),
        #[cfg(feature = "export")]
        Commands::Compile(args) => block_on(crate::compile::compile_main(args)),

        Commands::Completion(args) => crate::completion::completion_main(args),
        Commands::GenerateScript(args) => crate::generate_script::generate_script_main(args),

        #[cfg(feature = "lock")]
        Commands::Doc(cmds) => crate::doc::doc_main(cmds),
        #[cfg(feature = "lock")]
        Commands::Task(cmds) => crate::task::task_main(cmds),

        Commands::Cov(args) => crate::cov::cov_main(args),
        Commands::Test(args) => block_on(crate::test::test_main(args)),
    }
}
