//! CLI entrypoint for Tinymist documentation support tooling.

use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
struct Args {
    /// The generated JSON file consumed by the documentation sources.
    #[clap(long)]
    output: PathBuf,

    /// Fail instead of writing if the generated JSON differs from disk.
    #[clap(long)]
    check: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    tinymist_docs_tool::write_inventory_json(&args.output, args.check)
}
