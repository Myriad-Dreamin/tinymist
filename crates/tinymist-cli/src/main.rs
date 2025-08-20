#![doc = include_str!("../README.md")]

mod args;
#[cfg(feature = "export")]
mod compile;
mod completion;
mod cov;
#[cfg(feature = "dap")]
mod dap;
mod generate_script;
mod lsp;
#[cfg(feature = "preview")]
mod preview;
mod query;
mod test;
mod trace_lsp;
mod utils;

#[cfg(feature = "lock")]
mod doc;
#[cfg(feature = "lock")]
mod task;

use core::fmt;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use clap::Parser;
use parking_lot::Mutex;
use sync_ls::{
    GetMessageKind, LsHook, LspClientRoot, LspResult, Message, RequestId, TConnectionTx,
};
use tinymist::LONG_VERSION;
use tinymist::project::DocCommands;
use tinymist::world::system::print_diagnostics;
use tinymist::world::{DiagnosticFormat, SourceWorld};
#[cfg(feature = "l10n")]
use tinymist_l10n::{load_translations, set_translations};
use tinymist_std::hash::{FxBuildHasher, FxHashMap};
use tinymist_std::{bail, error::prelude::*};
use typst::ecow::EcoString;

use crate::compile::CompileArgs;

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
#[clap(name = "tinymist", author, version, about, long_version(LONG_VERSION.as_str()))]
struct CliArguments {
    /// Mode of the binary
    #[clap(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Clone, clap::Subcommand)]
#[clap(rename_all = "kebab-case")]
enum Commands {
    /// Probes existence (Nop run)
    Probe,

    /// Generates completion script to stdout
    Completion(crate::completion::ShellCompletionArgs),
    /// Runs language server
    Lsp(crate::lsp::LspArgs),
    /// Runs debug adapter
    #[cfg(feature = "dap")]
    Dap(crate::dap::DapArgs),
    /// Runs language server for tracing some typst program.
    #[clap(hide(true))]
    TraceLsp(crate::trace_lsp::TraceLspArgs),
    /// Runs preview server
    #[cfg(feature = "preview")]
    Preview(tinymist::tool::preview::PreviewCliArgs),

    /// Execute a document and collect coverage
    #[clap(hide(true))] // still in development
    Cov(crate::cov::CovArgs),
    /// Test a document and gives summary
    Test(crate::test::TestArgs),
    /// Runs compile command like `typst-cli compile`
    Compile(CompileArgs),
    /// Generates build script for compilation
    #[clap(hide(true))] // still in development
    GenerateScript(crate::generate_script::GenerateScriptArgs),
    /// Runs language query
    #[clap(hide(true))] // still in development
    #[clap(subcommand)]
    Query(crate::query::QueryCommands),
    /// Runs documents
    #[clap(hide(true))] // still in development
    #[clap(subcommand)]
    Doc(DocCommands),
    /// Runs tasks
    #[cfg(feature = "lock")]
    #[clap(hide(true))] // still in development
    #[clap(subcommand)]
    Task(crate::task::TaskCommands),
}

/// The main entry point.
fn main() -> Result<()> {
    // The root allocator for heap memory profiling.
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    // Parses command line arguments
    let args = CliArguments::parse();

    // Probes soon to avoid other initializations causing errors
    if matches!(args.command, Some(Commands::Probe)) {
        return Ok(());
    }

    // Loads translations
    #[cfg(feature = "l10n")]
    set_translations(load_translations(tinymist_assets::L10N_DATA)?);

    // Starts logging
    let _ = {
        use log::LevelFilter::*;

        let is_transient_cmd = matches!(args.command, Some(Commands::Compile(..)));
        let is_test_no_verbose =
            matches!(&args.command, Some(Commands::Test(test)) if !test.verbose);
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

    match args
        .command
        .unwrap_or_else(|| Commands::Lsp(Default::default()))
    {
        Commands::Probe => Ok(()),

        Commands::Completion(args) => crate::completion::completion_main(args),
        Commands::Lsp(args) => crate::lsp::lsp_main(args),
        #[cfg(feature = "dap")]
        Commands::Dap(args) => crate::dap::dap_main(args),
        Commands::TraceLsp(args) => crate::trace_lsp::trace_lsp_main(args),
        Commands::Query(cmds) => crate::query::query_main(cmds),

        #[cfg(feature = "preview")]
        Commands::Preview(args) => block_on(crate::preview::preview_main(args)),

        #[cfg(feature = "export")]
        Commands::Compile(args) => block_on(crate::compile::compile_main(args)),
        Commands::GenerateScript(args) => crate::generate_script::generate_script_main(args),
        #[cfg(feature = "lock")]
        Commands::Doc(cmds) => crate::doc::doc_main(cmds),
        #[cfg(feature = "lock")]
        Commands::Task(cmds) => crate::task::task_main(cmds),

        Commands::Cov(args) => crate::cov::cov_main(args),
        Commands::Test(args) => block_on(crate::test::test_main(args)),
    }
}

/// Creates a new language server host.
fn client_root<M: TryFrom<Message, Error = anyhow::Error> + GetMessageKind>(
    sender: TConnectionTx<M>,
) -> LspClientRoot {
    LspClientRoot::new(RUNTIMES.tokio_runtime.handle().clone(), sender)
        .with_hook(Arc::new(TypstLsHook::default()))
}

#[derive(Default)]
struct TypstLsHook(Mutex<FxHashMap<RequestId, typst_timing::TimingScope>>);

impl fmt::Debug for TypstLsHook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TypstLsHook").finish()
    }
}

impl LsHook for TypstLsHook {
    fn start_request(&self, req_id: &RequestId, method: &str) {
        ().start_request(req_id, method);

        if let Some(scope) = typst_timing::TimingScope::new(static_str(method)) {
            let mut map = self.0.lock();
            map.insert(req_id.clone(), scope);
        }
    }

    fn stop_request(
        &self,
        req_id: &RequestId,
        method: &str,
        received_at: tinymist_std::time::Instant,
    ) {
        ().stop_request(req_id, method, received_at);

        if let Some(scope) = self.0.lock().remove(req_id) {
            let _ = scope;
        }
    }

    fn start_notification(&self, method: &str) {
        ().start_notification(method);
    }

    fn stop_notification(
        &self,
        method: &str,
        received_at: tinymist_std::time::Instant,
        result: LspResult<()>,
    ) {
        ().stop_notification(method, received_at, result);
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    RUNTIMES.tokio_runtime.block_on(future)
}

fn static_str(s: &str) -> &'static str {
    static STRS: Mutex<FxHashMap<EcoString, &'static str>> =
        Mutex::new(HashMap::with_hasher(FxBuildHasher));

    let mut strs = STRS.lock();
    if let Some(&s) = strs.get(s) {
        return s;
    }

    let static_ref: &'static str = String::from(s).leak();
    strs.insert(static_ref.into(), static_ref);
    static_ref
}

fn print_diag_or_error<T>(world: &impl SourceWorld, result: Result<T>) -> Result<T> {
    match result {
        Ok(v) => Ok(v),
        Err(err) => {
            if let Some(diagnostics) = err.diagnostics() {
                print_diagnostics(world, diagnostics.iter(), DiagnosticFormat::Human)
                    .context_ut("print diagnostics")?;
                bail!("");
            }

            Err(err)
        }
    }
}
