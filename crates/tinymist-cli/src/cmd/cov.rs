use std::path::Path;

use reflexo_typst::TypstPagedDocument;
use tinymist::CompileOnceArgs;
use tinymist_project::WorldProvider;
use tinymist_std::error::prelude::*;

use crate::print_diag_or_error;

/// Coverage Testing arguments
#[derive(Debug, Clone, clap::Parser)]
pub struct CovArgs {
    /// The argument to compile once.
    #[clap(flatten)]
    pub compile: CompileOnceArgs,
}

/// Runs coverage test on a document
pub fn cov_main(args: CovArgs) -> Result<()> {
    // Prepares for the compilation
    let universe = args.compile.resolve()?;
    let world = universe.snapshot();

    let result = Ok(()).and_then(|_| -> Result<()> {
        let res = tinymist_debug::collect_coverage::<TypstPagedDocument, _>(&world)?;
        let cov_path = Path::new("target/coverage.json");
        let res = serde_json::to_string(&res.to_json(&world)).context("coverage")?;

        std::fs::create_dir_all(cov_path.parent().context("parent")?).context("create coverage")?;
        std::fs::write(cov_path, res).context("write coverage")?;

        Ok(())
    });

    print_diag_or_error(&world, result)
}
