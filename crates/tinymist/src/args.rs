use once_cell::sync::Lazy;
use sync_lsp::transport::MirrorArgs;

use tinymist::{CompileFontArgs, CompileOnceArgs};

#[derive(Debug, Clone, clap::Parser)]
#[clap(name = "tinymist", author, version, about, long_version(LONG_VERSION.as_str()))]
pub struct CliArguments {
    /// Mode of the binary
    #[clap(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum Commands {
    /// Generates completion script to stdout
    Completion(ShellCompletionArgs),
    /// Runs language server
    Lsp(LspArgs),
    /// Runs language server for tracing some typst program.
    #[clap(hide(true))]
    TraceLsp(CompileArgs),
    /// Runs preview server
    #[cfg(feature = "preview")]
    Preview(tinymist::tool::preview::PreviewCliArgs),
    /// Probes existence (Nop run)
    Probe,
}

impl Default for Commands {
    fn default() -> Self {
        Self::Lsp(Default::default())
    }
}

#[derive(Debug, Clone, clap::Parser)]
pub struct ShellCompletionArgs {
    /// The shell to generate the completion script for. If not provided, it
    /// will be inferred from the environment.
    #[clap(value_enum)]
    pub shell: Option<clap_complete::Shell>,
}

#[derive(Debug, Clone, Default, clap::Parser)]
pub struct CompileArgs {
    #[clap(long, default_value = "false")]
    pub persist: bool,
    #[clap(flatten)]
    pub mirror: MirrorArgs,
    #[clap(flatten)]
    pub compile: CompileOnceArgs,
}

#[derive(Debug, Clone, Default, clap::Parser)]
pub struct LspArgs {
    #[clap(flatten)]
    pub mirror: MirrorArgs,
    #[clap(flatten)]
    pub font: CompileFontArgs,
}

pub static LONG_VERSION: Lazy<String> = Lazy::new(|| {
    format!(
        "
Build Timestamp:     {}
Build Git Describe:  {}
Commit SHA:          {}
Commit Date:         {}
Commit Branch:       {}
Cargo Target Triple: {}
Typst Version:       {}
",
        env!("VERGEN_BUILD_TIMESTAMP"),
        env!("VERGEN_GIT_DESCRIBE"),
        option_env!("VERGEN_GIT_SHA").unwrap_or("None"),
        option_env!("VERGEN_GIT_COMMIT_TIMESTAMP").unwrap_or("None"),
        option_env!("VERGEN_GIT_BRANCH").unwrap_or("None"),
        env!("VERGEN_CARGO_TARGET_TRIPLE"),
        env!("TYPST_VERSION"),
    )
});
