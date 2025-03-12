use std::path::Path;

use sync_lsp::transport::MirrorArgs;
use tinymist::project::DocCommands;
use tinymist::tool::project::{CompileArgs, GenerateScriptArgs, TaskCommands};
use tinymist::{CompileFontArgs, CompileOnceArgs};
use tinymist_core::LONG_VERSION;

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
    Completion(ShellCompletionArgs),
    /// Runs language server
    Lsp(LspArgs),
    /// Runs language server for tracing some typst program.
    #[clap(hide(true))]
    TraceLsp(TraceLspArgs),
    /// Runs preview server
    #[cfg(feature = "preview")]
    Preview(tinymist::tool::preview::PreviewCliArgs),

    /// Execute a document and collect coverage
    #[clap(hide(true))] // still in development
    Cov(CompileOnceArgs),
    /// Test a document and gives summary
    #[clap(hide(true))] // still in development
    Test(CompileOnceArgs),
    /// Runs compile command like `typst-cli compile`
    Compile(CompileArgs),
    /// Generates build script for compilation
    #[clap(hide(true))] // still in development
    GenerateScript(GenerateScriptArgs),
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
        Self::Lsp(LspArgs::default())
    }
}

#[derive(Debug, Clone, clap::Parser)]
pub struct ShellCompletionArgs {
    /// The shell to generate the completion script for. If not provided, it
    /// will be inferred from the environment.
    #[clap(value_enum)]
    pub shell: Option<Shell>,
}

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, clap::ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum Shell {
    Bash,
    Elvish,
    Fig,
    Fish,
    PowerShell,
    Zsh,
    Nushell,
}

impl Shell {
    pub fn from_env() -> Option<Self> {
        if let Some(env_shell) = std::env::var_os("SHELL") {
            let name = Path::new(&env_shell).file_stem()?.to_str()?;

            match name {
                "bash" => Some(Shell::Bash),
                "zsh" => Some(Shell::Zsh),
                "fig" => Some(Shell::Fig),
                "fish" => Some(Shell::Fish),
                "elvish" => Some(Shell::Elvish),
                "powershell" | "powershell_ise" => Some(Shell::PowerShell),
                "nushell" => Some(Shell::Nushell),
                _ => None,
            }
        } else if cfg!(windows) {
            Some(Shell::PowerShell)
        } else {
            None
        }
    }
}

impl clap_complete::Generator for Shell {
    fn file_name(&self, name: &str) -> String {
        use clap_complete::shells::{Bash, Elvish, Fish, PowerShell, Zsh};
        use clap_complete_fig::Fig;
        use clap_complete_nushell::Nushell;

        match self {
            Shell::Bash => Bash.file_name(name),
            Shell::Elvish => Elvish.file_name(name),
            Shell::Fig => Fig.file_name(name),
            Shell::Fish => Fish.file_name(name),
            Shell::PowerShell => PowerShell.file_name(name),
            Shell::Zsh => Zsh.file_name(name),
            Shell::Nushell => Nushell.file_name(name),
        }
    }

    fn generate(&self, cmd: &clap::Command, buf: &mut dyn std::io::Write) {
        use clap_complete::shells::{Bash, Elvish, Fish, PowerShell, Zsh};
        use clap_complete_fig::Fig;
        use clap_complete_nushell::Nushell;

        match self {
            Shell::Bash => Bash.generate(cmd, buf),
            Shell::Elvish => Elvish.generate(cmd, buf),
            Shell::Fig => Fig.generate(cmd, buf),
            Shell::Fish => Fish.generate(cmd, buf),
            Shell::PowerShell => PowerShell.generate(cmd, buf),
            Shell::Zsh => Zsh.generate(cmd, buf),
            Shell::Nushell => Nushell.generate(cmd, buf),
        }
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

#[derive(Debug, Clone, Default, clap::Parser)]
pub struct LspArgs {
    #[clap(flatten)]
    pub mirror: MirrorArgs,
    #[clap(flatten)]
    pub font: CompileFontArgs,
}

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
