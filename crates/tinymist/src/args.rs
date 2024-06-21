use once_cell::sync::Lazy;
use tinymist::transport::MirrorArgs;

use tinymist::compiler_init::{CompileOnceArgs, FontArgs};
use tinymist::preview::PreviewCliArgs;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "clap", derive(clap::Parser))]
#[cfg_attr(feature = "clap", clap(name = "tinymist", author, version, about, long_version(LONG_VERSION.as_str())))]
pub struct CliArguments {
    /// Mode of the binary
    #[cfg_attr(feature = "clap", clap(subcommand))]
    pub command: Option<Commands>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "clap", derive(clap::Subcommand))]
pub enum Commands {
    /// Run Language Server
    Lsp(LspArgs),
    /// Run Compile Server
    Compile(CompileArgs),
    /// Run Preview Server
    Preview(PreviewCliArgs),
    /// Probe
    Probe,
}

impl Default for Commands {
    fn default() -> Self {
        Self::Lsp(Default::default())
    }
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "clap", derive(clap::Parser))]
pub struct CompileArgs {
    #[cfg_attr(feature = "clap", clap(long, default_value = "false"))]
    pub persist: bool,
    #[cfg_attr(feature = "clap", clap(flatten))]
    pub mirror: MirrorArgs,
    #[cfg_attr(feature = "clap", clap(flatten))]
    pub compile: CompileOnceArgs,
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "clap", derive(clap::Parser))]
pub struct LspArgs {
    #[cfg_attr(feature = "clap", clap(flatten))]
    pub mirror: MirrorArgs,
    #[cfg_attr(feature = "clap", clap(flatten))]
    pub font: FontArgs,
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
