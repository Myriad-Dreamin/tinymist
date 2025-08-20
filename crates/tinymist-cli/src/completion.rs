use std::io;

use clap::CommandFactory;
use clap_complete::{Shell, generate};
use tinymist_std::error::prelude::*;

use crate::args::CliArguments;

#[derive(Debug, Clone, clap::Parser)]
pub struct ShellCompletionArgs {
    /// The shell to generate the completion script for. If not provided, it
    /// will be inferred from the environment.
    #[clap(value_enum)]
    pub shell: Option<Shell>,
}

/// Generates completion script to stdout.
pub fn completion_main(args: ShellCompletionArgs) -> Result<()> {
    let Some(shell) = args.shell.or_else(Shell::from_env) else {
        tinymist_std::bail!("could not infer shell");
    };

    let mut cmd = CliArguments::command();
    generate(shell, &mut cmd, "tinymist", &mut io::stdout());

    Ok(())
}
