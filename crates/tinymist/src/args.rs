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
    /// Run Language Server
    Lsp(LspArgs),
    /// Run Language Server for tracing some typst program.
    #[clap(hide(true))]
    TraceLsp(CompileArgs),
    /// Run Preview Server
    #[cfg(feature = "preview")]
    Preview(tinymist::tool::preview::PreviewCliArgs),
    /// Probe
    Probe,
}

impl Default for Commands {
    fn default() -> Self {
        Self::Lsp(Default::default())
    }
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
