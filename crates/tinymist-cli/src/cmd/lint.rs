use tinymist::project::CompiledArtifact;
use tinymist::world::DiagnosticFormat;
use tinymist::world::system::print_diagnostics;
use tinymist_project::WorldProvider;
use tinymist_std::error::prelude::*;
use typlite::CompileOnceArgs;

/// Run Tinymist lint checks.
#[derive(Debug, Clone, clap::Parser)]
pub struct LintArgs {
    /// Compile a document once before linting.
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// Configure how diagnostics are printed.
    #[clap(long = "diagnostic-format", value_enum, default_value_t)]
    pub diagnostic_format: LintDiagnosticFormat,
}

#[derive(Debug, Copy, Clone, Default, clap::ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum LintDiagnosticFormat {
    /// Human-readable diagnostics.
    #[default]
    Human,
    /// Short Unix-style diagnostics.
    Short,
}

impl From<LintDiagnosticFormat> for DiagnosticFormat {
    fn from(format: LintDiagnosticFormat) -> Self {
        match format {
            LintDiagnosticFormat::Human => DiagnosticFormat::Human,
            LintDiagnosticFormat::Short => DiagnosticFormat::Short,
        }
    }
}

/// Runs Tinymist lint checks.
pub fn lint_main(args: LintArgs) -> Result<()> {
    let analysis = crate::query::default_analysis();
    let verse = args.compile.resolve()?;
    let graph = verse.computation();
    let compiled = CompiledArtifact::from_graph(graph, false);
    let compiler_diagnostics = compiled.diagnostics().cloned().collect::<Vec<_>>();

    let lint_diagnostics = analysis
        .query_snapshot(compiled.graph.clone(), None)
        .run_analysis(|ctx| {
            tinymist_query::collect_lint_diagnostics(ctx, compiler_diagnostics.iter())
        })?;
    let has_lint_diagnostics = !lint_diagnostics.is_empty();

    let mut diagnostics = compiler_diagnostics;
    diagnostics.extend(lint_diagnostics);
    print_diagnostics(
        compiled.world(),
        diagnostics.iter(),
        args.diagnostic_format.into(),
    )
    .context_ut("print diagnostics")?;

    if compiled.has_errors() || has_lint_diagnostics {
        std::process::exit(1);
    }

    Ok(())
}
