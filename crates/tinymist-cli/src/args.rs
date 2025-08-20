use sync_ls::transport::MirrorArgs;
use tinymist::LONG_VERSION;
use tinymist::project::DocCommands;
use tinymist::{CompileFontArgs, CompileOnceArgs};

#[cfg(feature = "preview")]
use tinymist::tool::preview::PreviewArgs;
#[cfg(feature = "preview")]
use tinymist_project::DocNewArgs;
#[cfg(feature = "preview")]
use tinymist_task::TaskWhen;

use crate::compile::CompileArgs;

#[derive(Debug, Clone, clap::Parser)]
#[clap(name = "tinymist", author, version, about, long_version(LONG_VERSION.as_str()))]
pub struct CliArguments {
    /// Mode of the binary
    #[clap(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Clone, clap::Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum Commands {
    /// Probes existence (Nop run)
    Probe,

    /// Generates completion script to stdout
    Completion(crate::completion::ShellCompletionArgs),
    /// Runs language server
    Lsp(crate::lsp::LspArgs),
    /// Runs debug adapter
    Dap(DapArgs),
    /// Runs language server for tracing some typst program.
    #[clap(hide(true))]
    TraceLsp(TraceLspArgs),
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
    Query(QueryCommands),
    /// Runs documents
    #[clap(hide(true))] // still in development
    #[clap(subcommand)]
    Doc(DocCommands),
    /// Runs tasks
    #[clap(hide(true))] // still in development
    #[clap(subcommand)]
    Task(TaskCommands),
}

impl Default for Commands {
    fn default() -> Self {
        Self::Lsp(crate::lsp::LspArgs::default())
    }
}

#[derive(Debug, Clone, Default, clap::Parser)]
pub struct TraceLspArgs {
    #[clap(long, default_value = "false")]
    pub persist: bool,
    // lsp or http
    #[clap(long, default_value = "lsp")]
    pub rpc_kind: String,
    #[clap(flatten)]
    pub mirror: MirrorArgs,
    #[clap(flatten)]
    pub compile: CompileOnceArgs,
}

pub type DapArgs = LspArgs;

#[derive(Debug, Clone, clap::Subcommand)]
#[clap(rename_all = "camelCase")]
pub enum QueryCommands {
    /// Get the documentation for a specific package.
    PackageDocs(PackageDocsArgs),
    /// Check a specific package.
    CheckPackage(PackageDocsArgs),
}

#[derive(Debug, Clone, clap::Parser)]
pub struct PackageDocsArgs {
    /// The path of the package to request docs for.
    #[clap(long)]
    pub path: Option<String>,
    /// The package of the package to request docs for.
    #[clap(long)]
    pub id: String,
    /// The output path for the requested docs.
    #[clap(short, long)]
    pub output: String,
    // /// The format of requested docs.
    // #[clap(long)]
    // pub format: Option<QueryDocsFormat>,
}

#[derive(Debug, Clone, Default, clap::ValueEnum)]
#[clap(rename_all = "camelCase")]
pub enum QueryDocsFormat {
    #[default]
    Json,
    Markdown,
}

/// Project task commands.
#[derive(Debug, Clone, clap::Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum TaskCommands {
    /// Declare a preview task.
    #[cfg(feature = "preview")]
    Preview(TaskPreviewArgs),
}

/// Declare an lsp task.
#[derive(Debug, Clone, clap::Parser)]
#[cfg(feature = "preview")]
pub struct TaskPreviewArgs {
    /// Argument to identify a project.
    #[clap(flatten)]
    pub declare: DocNewArgs,

    /// Name a task.
    #[clap(long = "task")]
    pub task_name: Option<String>,

    /// When to run the task
    #[arg(long = "when")]
    pub when: Option<TaskWhen>,

    /// Preview arguments
    #[clap(flatten)]
    pub preview: PreviewArgs,
}
