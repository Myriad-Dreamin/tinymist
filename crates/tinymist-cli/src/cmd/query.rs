use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use reflexo_typst::package::PackageSpec;
use tinymist::Config;
use tinymist_project::WorldProvider;
use tinymist_query::analysis::Analysis;
use tinymist_query::package::PackageInfo;
use tinymist_std::error::prelude::*;
use typlite::CompileOnceArgs;

/// The commands for language server queries.
#[derive(Debug, Clone, clap::Subcommand)]
#[clap(rename_all = "camelCase")]
pub enum QueryCommands {
    /// Get the lsif for a specific package.
    Lsif(QueryLsifArgs),
    /// Get the documentation for a specific package.
    PackageDocs(PackageDocsArgs),
    /// Check a specific package.
    CheckPackage(PackageDocsArgs),
}

#[derive(Debug, Clone, clap::Parser)]
pub struct QueryLsifArgs {
    /// Compile a document once before querying.
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// The path of the package to request lsif for.
    #[clap(long)]
    pub path: Option<String>,
    /// The package of the package to request lsif for.
    #[clap(long)]
    pub id: String,
    /// The output path for the requested lsif.
    #[clap(short, long)]
    pub output: String,
    // /// The format of requested lsif.
    // #[clap(long)]
    // pub format: Option<QueryDocsFormat>,
}

#[derive(Debug, Clone, clap::Parser)]
pub struct PackageDocsArgs {
    /// Compile a document once before querying.
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

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

/// The main entry point for language server queries.
pub fn query_main(mut cmds: QueryCommands) -> Result<()> {
    use tinymist_project::package::PackageRegistry;
    let (config, _) = Config::extract_lsp_params(Default::default(), Default::default());
    let const_config = &config.const_config;
    let analysis = Arc::new(Analysis {
        position_encoding: const_config.position_encoding,
        allow_overlapping_token: const_config.tokens_overlapping_token_support,
        allow_multiline_token: const_config.tokens_multiline_token_support,
        remove_html: !config.support_html_in_markdown,
        support_client_codelens: true,
        extended_code_action: config.extended_code_action,
        completion_feat: config.completion.clone(),
        color_theme: match config.color_theme.as_deref() {
            Some("dark") => tinymist_query::ColorTheme::Dark,
            _ => tinymist_query::ColorTheme::Light,
        },
        lint: config.lint.when().clone(),
        unused: config.lint.unused_config(),
        periscope: None,
        local_packages: Arc::default(),
        tokens_caches: Arc::default(),
        workers: Default::default(),
        caches: Default::default(),
        analysis_rev_cache: Arc::default(),
        stats: Arc::default(),
    });

    let compile = match &mut cmds {
        QueryCommands::Lsif(args) => &mut args.compile,
        QueryCommands::PackageDocs(args) => &mut args.compile,
        QueryCommands::CheckPackage(args) => &mut args.compile,
    };
    if compile.input.is_none() {
        compile.input = Some("main.typ".to_string());
    }
    let verse = compile.resolve()?;
    let snap = verse.computation();
    let snap = analysis.query_snapshot(snap, None);

    let (id, path) = match &cmds {
        QueryCommands::Lsif(args) => (&args.id, &args.path),
        QueryCommands::PackageDocs(args) => (&args.id, &args.path),
        QueryCommands::CheckPackage(args) => (&args.id, &args.path),
    };
    let pkg = PackageSpec::from_str(id).unwrap();
    let path = path.as_ref().map(PathBuf::from);
    let path = path.unwrap_or_else(|| snap.registry().resolve(&pkg).unwrap().as_ref().into());

    let info = PackageInfo {
        path,
        namespace: pkg.namespace,
        name: pkg.name,
        version: pkg.version.to_string(),
    };

    match cmds {
        QueryCommands::Lsif(args) => {
            let res = snap.run_within_package(&info, move |a| {
                let knowledge = tinymist_query::index::knowledge(a)
                    .map_err(map_string_err("failed to generate index"))?;
                Ok(knowledge.bind(a.shared()).to_string())
            })?;

            let output_path = Path::new(&args.output);
            std::fs::write(output_path, res).context_ut("failed to write lsif output")?;
        }
        QueryCommands::PackageDocs(args) => {
            let res = snap.run_within_package(&info, |a| {
                let doc = tinymist_query::docs::package_docs(a, &info)
                    .map_err(map_string_err("failed to generate docs"))?;
                tinymist_query::docs::package_docs_md(&doc)
                    .map_err(map_string_err("failed to generate docs"))
            })?;

            let output_path = Path::new(&args.output);
            std::fs::write(output_path, res).context_ut("failed to write package docs")?;
        }
        QueryCommands::CheckPackage(_args) => {
            snap.run_within_package(&info, |a| {
                tinymist_query::package::check_package(a, &info)
                    .map_err(map_string_err("failed to check package"))
            })?;
        }
    };

    Ok(())
}

#[derive(Debug, Clone, Default, clap::ValueEnum)]
#[clap(rename_all = "camelCase")]
enum QueryDocsFormat {
    #[default]
    Json,
    Markdown,
}
