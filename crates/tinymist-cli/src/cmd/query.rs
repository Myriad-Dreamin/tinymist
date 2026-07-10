use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use protobuf::Message;
use reflexo_typst::package::PackageSpec;
use serde::Serialize;
use tinymist::Config;
use tinymist_project::WorldProvider;
use tinymist_query::analysis::Analysis;
use tinymist_query::index::ScipPublicApi;
use tinymist_query::package::PackageInfo;
use tinymist_std::error::prelude::*;
use tinymist_std::fs::paths::write_atomic;
use typlite::CompileOnceArgs;

/// The commands for language server queries.
#[derive(Debug, Clone, clap::Subcommand)]
#[clap(rename_all = "camelCase")]
pub enum QueryCommands {
    /// Get the lsif for a specific package.
    Lsif(QueryLsifArgs),
    /// Get the SCIP index for a specific package.
    Scip(QueryScipArgs),
    /// Get the documentation for a specific package.
    PackageDocs(PackageDocsArgs),
    /// Check a specific package.
    CheckPackage(PackageDocsArgs),
    /// Dump package scopes and type-checker results.
    TyckScope(QueryTyckScopeArgs),
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
pub struct QueryScipArgs {
    /// Compile a document once before querying.
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// The path of the package to request SCIP for.
    #[clap(long)]
    pub path: Option<String>,
    /// The package to request SCIP for.
    #[clap(long)]
    pub id: String,
    /// The output path for the requested SCIP protobuf.
    #[clap(short, long)]
    pub output: String,
    /// The output path for structured analysis statistics.
    #[clap(long, hide(true))]
    pub stats_output: Option<String>,
    /// The output path for structured SCIP index statistics.
    #[clap(long, hide(true))]
    pub index_summary_output: Option<String>,
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

#[derive(Debug, Clone, clap::Parser)]
pub struct QueryTyckScopeArgs {
    /// Compile a document once before querying.
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// The path of the package to request tyck scopes for.
    #[clap(long)]
    pub path: Option<String>,
    /// The package to request tyck scopes for.
    #[clap(long)]
    pub id: String,
    /// The output path for the requested tyck scope dump.
    #[clap(short, long)]
    pub output: String,
    /// Maximum characters kept for each dumped type string. Set to 0 to keep full strings.
    #[clap(long, default_value_t = 8192)]
    pub max_type_chars: usize,
}

/// Creates the default analysis context for CLI query-style commands.
pub fn default_analysis() -> Arc<Analysis> {
    let (config, _) = Config::extract_lsp_params(Default::default(), Default::default());
    let const_config = &config.const_config;
    Arc::new(Analysis {
        position_encoding: const_config.position_encoding,
        allow_overlapping_token: const_config.tokens_overlapping_token_support,
        allow_multiline_token: const_config.tokens_multiline_token_support,
        remove_html: !config.support_html_in_markdown,
        support_client_codelens: config.support_client_codelens,
        extended_code_action: config.extended_code_action,
        completion_feat: config.completion.clone(),
        color_theme: match config.color_theme.as_deref() {
            Some("dark") => tinymist_query::ColorTheme::Dark,
            _ => tinymist_query::ColorTheme::Light,
        },
        lint: config.lint.when().clone(),
        periscope: None,
        local_packages: Arc::default(),
        tokens_caches: Arc::default(),
        workers: Default::default(),
        caches: Default::default(),
        analysis_rev_cache: Arc::default(),
        stats: Arc::default(),
    })
}

/// The main entry point for language server queries.
pub fn query_main(mut cmds: QueryCommands) -> Result<()> {
    use tinymist_project::package::PackageRegistry;
    let analysis = default_analysis();

    let compile = match &mut cmds {
        QueryCommands::Lsif(args) => &mut args.compile,
        QueryCommands::Scip(args) => &mut args.compile,
        QueryCommands::PackageDocs(args) => &mut args.compile,
        QueryCommands::CheckPackage(args) => &mut args.compile,
        QueryCommands::TyckScope(args) => &mut args.compile,
    };
    if compile.input.is_none() {
        compile.input = Some("main.typ".to_string());
    }
    let verse = compile.resolve()?;
    let snap = verse.computation();
    let snap = analysis.clone().query_snapshot(snap, None);

    let (id, path) = match &cmds {
        QueryCommands::Lsif(args) => (&args.id, &args.path),
        QueryCommands::Scip(args) => (&args.id, &args.path),
        QueryCommands::PackageDocs(args) => (&args.id, &args.path),
        QueryCommands::CheckPackage(args) => (&args.id, &args.path),
        QueryCommands::TyckScope(args) => (&args.id, &args.path),
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

            write_output(Path::new(&args.output), res, "failed to write lsif output")?;
        }
        QueryCommands::Scip(args) => {
            let (bytes, summary) = snap.run_within_package(&info, |a| {
                let docs = tinymist_query::docs::package_docs(a, &info)
                    .map_err(map_string_err("failed to generate docs"))?;
                let public_api = docs.scip_public_api();
                let knowledge = tinymist_query::index::knowledge(a)
                    .map_err(map_string_err("failed to generate index"))?;
                let index = knowledge
                    .bind(a.shared())
                    .to_scip_index_with_public_api(&public_api)?;
                let summary = ScipIndexSummary::from_index(&info, &index, &public_api);
                let bytes = index
                    .write_to_bytes()
                    .context_ut("failed to serialize SCIP index")?;
                Ok((bytes, summary))
            })?;

            write_output(
                Path::new(&args.output),
                bytes,
                "failed to write SCIP output",
            )?;
            if let Some(stats_output) = args.stats_output {
                write_analysis_stats(&analysis, Path::new(&stats_output))?;
            }
            if let Some(index_summary_output) = args.index_summary_output {
                let summary = serde_json::to_vec_pretty(&summary)
                    .context_ut("failed to serialize SCIP summary")?;
                write_output(
                    Path::new(&index_summary_output),
                    summary,
                    "failed to write SCIP summary",
                )?;
            }
        }
        QueryCommands::PackageDocs(args) => {
            let res = snap.run_within_package(&info, |a| {
                let doc = tinymist_query::docs::package_docs(a, &info)
                    .map_err(map_string_err("failed to generate docs"))?;
                tinymist_query::docs::package_docs_md(&doc)
                    .map_err(map_string_err("failed to generate docs"))
            })?;

            write_output(Path::new(&args.output), res, "failed to write package docs")?;
        }
        QueryCommands::CheckPackage(_args) => {
            snap.run_within_package(&info, |a| {
                tinymist_query::package::check_package(a, &info)
                    .map_err(map_string_err("failed to check package"))
            })?;
        }
        QueryCommands::TyckScope(args) => {
            let options = tinymist_query::package::PackageTyckDumpOptions {
                max_type_chars: (args.max_type_chars > 0).then_some(args.max_type_chars),
            };
            let res = snap.run_within_package(&info, |a| {
                tinymist_query::package::package_tyck_scope(a, &info, options)
                    .map_err(map_string_err("failed to dump package tyck scopes"))
            })?;
            let res = serde_json::to_vec_pretty(&res)
                .context_ut("failed to serialize package tyck scope dump")?;

            write_output(
                Path::new(&args.output),
                res,
                "failed to write package tyck scope dump",
            )?;
        }
    };

    Ok(())
}

fn write_output(path: &Path, data: impl AsRef<[u8]>, message: &'static str) -> Result<()> {
    let path = if path
        .parent()
        .is_some_and(|parent| parent.as_os_str().is_empty())
    {
        Path::new(".").join(path)
    } else {
        path.to_path_buf()
    };
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).context_ut("failed to create output directory")?;
    }
    write_atomic(path, data).context_ut(message)
}

fn write_analysis_stats(analysis: &Analysis, path: &Path) -> Result<()> {
    let stats = serde_json::to_vec_pretty(&analysis.report_query_stats_json())
        .context_ut("failed to serialize analysis stats")?;
    write_output(path, stats, "failed to write stats output")
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScipIndexSummary {
    schema: u32,
    package: String,
    documents: usize,
    occurrences: usize,
    document_symbols: usize,
    external_symbols: usize,
    relationships: usize,
    public_modules: usize,
    public_symbols: usize,
}

impl ScipIndexSummary {
    fn from_index(
        info: &PackageInfo,
        index: &scip::types::Index,
        public_api: &ScipPublicApi,
    ) -> Self {
        let document_symbols = index
            .documents
            .iter()
            .map(|document| document.symbols.len())
            .sum();
        let document_relationships = index
            .documents
            .iter()
            .flat_map(|document| document.symbols.iter())
            .map(|symbol| symbol.relationships.len())
            .sum::<usize>();
        let external_relationships = index
            .external_symbols
            .iter()
            .map(|symbol| symbol.relationships.len())
            .sum::<usize>();

        Self {
            schema: 1,
            package: format!("@{}/{}:{}", info.namespace, info.name, info.version),
            documents: index.documents.len(),
            occurrences: index
                .documents
                .iter()
                .map(|document| document.occurrences.len())
                .sum(),
            document_symbols,
            external_symbols: index.external_symbols.len(),
            relationships: document_relationships + external_relationships,
            public_modules: public_api.modules.len(),
            public_symbols: public_api
                .modules
                .iter()
                .map(|module| module.public_symbols.len())
                .sum(),
        }
    }
}

#[derive(Debug, Clone, Default, clap::ValueEnum)]
#[clap(rename_all = "camelCase")]
enum QueryDocsFormat {
    #[default]
    Json,
    Markdown,
}
