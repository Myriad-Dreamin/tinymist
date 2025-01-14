use anyhow::Context;
use clap::Parser;
use tinymist_world::CompileOnceArgs;

/// Common arguments of crityp benchmark.
#[derive(Debug, Clone, Parser, Default)]
pub struct BenchArgs {
    /// Arguments for compiling the document once, compatible with `typst-cli
    /// compile`.
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// Path to output file for benchmarks
    #[clap(long, default_value = "target/crityp")]
    pub bench_output: String,
}

pub fn main() -> anyhow::Result<()> {
    // Parse command line arguments
    let args = BenchArgs::parse();

    let universe = args.compile.resolve()?;
    let mut world = universe.snapshot();

    let out_dir = std::env::current_dir()
        .context("cannot get current working directory")?
        .join(args.bench_output);
    let mut crit = criterion::Criterion::default().output_directory(&out_dir);

    crityp::bench(&mut crit, &mut world)?;

    crit.final_summary();

    Ok(())
}
