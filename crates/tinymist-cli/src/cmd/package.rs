//! Package-related CLI commands.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use clap::ValueHint;
use tinymist::project::{
    CompiledArtifact, ExportBundleTask, ExportTarget, Interrupt, PathPattern, ProjectTask, Scalar,
    TaskWhen, WorldComputeGraph,
};
use tinymist::tool::project::{ProjectOpts, start_project};
use tinymist::{CompileFontArgs, CompileOnceArgs, Config, ExportTask};
use tinymist_project::world::package::{PackageRegistry, PackageSpec};
use tinymist_project::{Feature, WorldProvider};
use tinymist_query::analysis::Analysis;
use tinymist_query::package::PackageInfo;
use tinymist_std::error::prelude::*;
use tinymist_std::fs::paths::copy_dir_all;
use typst::ecow::EcoString;
use typst::syntax::package::PackageManifest;

/// Package commands.
#[derive(Debug, Clone, clap::Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum PackageCommands {
    /// Generate package API docs.
    Docs(PackageDocsArgs),
}

/// Generate package API docs.
#[derive(Debug, Clone, clap::Parser)]
pub struct PackageDocsArgs {
    /// Regenerate and export docs when package sources change.
    #[clap(long)]
    pub watch: bool,

    /// Package namespace. Defaults to the namespace inferred from a local
    /// package layout, or `preview` for a plain package directory.
    #[clap(long)]
    pub namespace: Option<String>,

    /// Specify compile related arguments.
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// Output directory for generated bundle docs.
    #[clap(value_name = "OUTPUT", value_hint = ValueHint::DirPath)]
    pub output: Option<PathBuf>,
}

/// Main entry point for package commands.
pub async fn package_main(cmd: PackageCommands) -> Result<()> {
    match cmd {
        PackageCommands::Docs(args) => docs_main(args).await,
    }
}

async fn docs_main(args: PackageDocsArgs) -> Result<()> {
    let ctx = PackageDocsContext::new(args)?;

    if !ctx.args.watch {
        let analysis = Arc::new(Analysis::default());
        generate_and_export_docs(&ctx, analysis).await?;
        return Ok(());
    }

    let analysis = Arc::new(Analysis::default());
    let verse = ctx.package_compile_args().resolve()?;
    let ctx = Arc::new(ctx);
    let opts = ProjectOpts {
        handle: Some(tokio::runtime::Handle::current()),
        analysis: analysis.clone(),
        config: Config::default(),
        export_target: ExportTarget::Paged,
        #[cfg(feature = "preview")]
        preview: Default::default(),
    };

    let mut is_first = true;
    let result = start_project(verse, Some(opts), move |compiler, interrupt, next| {
        if let Interrupt::Compiled(artifact) = &interrupt {
            if artifact.has_errors() {
                eprintln!("package docs: package compile has errors; skip docs export");
            } else {
                let ctx = ctx.clone();
                let analysis = analysis.clone();
                tokio::spawn(async move {
                    let prefix = if is_first {
                        "package docs: exported"
                    } else {
                        "package docs: re-exported"
                    };
                    if let Err(err) = generate_and_export_docs(&ctx, analysis).await {
                        eprintln!("package docs: {err:?}");
                    } else {
                        eprintln!("{prefix} {}", ctx.dist.display());
                    }
                });
                is_first = false;
            }
        }

        next(compiler, interrupt)
    });

    let mut editor_rx = result.editor_rx;
    tokio::spawn(async move { while editor_rx.recv().await.is_some() {} });
    result.service.run().await;
    Ok(())
}

#[derive(Debug)]
struct PackageDocsContext {
    args: PackageDocsArgs,
    repo_root: PathBuf,
    package_root: PathBuf,
    package_info: PackageInfo,
    manifest: PackageManifest,
    staging_root: PathBuf,
    source_root: PathBuf,
    dist: PathBuf,
}

struct PackageDocsInput {
    package_root: PathBuf,
    namespace: Option<String>,
}

impl PackageDocsContext {
    fn new(args: PackageDocsArgs) -> Result<Self> {
        let repo_root = find_repo_root()?;
        let package_input = args
            .compile
            .input
            .as_deref()
            .context("package docs requires INPUT package path")?;
        let resolved_input = resolve_package_docs_input(&args, package_input)?;
        let package_root = resolved_input.package_root;
        let manifest_path = package_root.join("typst.toml");
        let manifest_data = std::fs::read_to_string(&manifest_path)
            .context_ut("failed to read package manifest")?;
        let manifest: PackageManifest = toml::from_str(&manifest_data)
            .map_err(map_string_err("failed to parse package manifest"))?;

        let namespace = resolved_input
            .namespace
            .or_else(|| args.namespace.clone())
            .or_else(|| infer_namespace(&package_root, &manifest))
            .unwrap_or_else(|| "preview".to_owned());
        let package_name = manifest.package.name.to_string();
        let package_version = manifest.package.version.to_string();
        let base = format!("{namespace}-{package_name}-{package_version}");
        let work_root = repo_root.join("target/package-docs").join(&base);
        let staging_root = work_root.join("packages");
        let source_root = work_root.join("bundle");
        let dist = args
            .output
            .as_deref()
            .map(absolutize)
            .transpose()?
            .unwrap_or_else(|| work_root.join("dist"));

        let package_info = PackageInfo {
            path: package_root.clone(),
            namespace: EcoString::from(namespace),
            name: manifest.package.name.clone(),
            version: package_version,
        };

        let ctx = Self {
            args,
            repo_root,
            package_root,
            package_info,
            manifest,
            staging_root,
            source_root,
            dist,
        };
        ctx.stage_package()?;
        Ok(ctx)
    }

    fn package_compile_args(&self) -> CompileOnceArgs {
        self.compile_args(
            self.package_root
                .join(self.manifest.package.entrypoint.as_str()),
            self.package_root.clone(),
        )
    }

    fn docs_compile_args(&self) -> CompileOnceArgs {
        self.compile_args(self.source_root.join("index.typ"), self.repo_root.clone())
    }

    fn query_compile_args(&self) -> CompileOnceArgs {
        self.compile_args(self.work_root().join("query.typ"), self.repo_root.clone())
    }

    fn compile_args(&self, input: PathBuf, root: PathBuf) -> CompileOnceArgs {
        let mut compile = self.args.compile.clone();
        compile.input = Some(input.to_string_lossy().into_owned());
        compile.root = Some(root);
        compile.font = self.compile_font_args();
        compile.package.package_path = Some(self.staging_root.clone());
        compile.features = self.typst_features();
        compile
    }

    fn compile_font_args(&self) -> CompileFontArgs {
        let mut font = self.args.compile.font.clone();
        let assets_fonts = self.repo_root.join("assets/fonts");
        if assets_fonts.is_dir() && !font.font_paths.iter().any(|p| p == &assets_fonts) {
            font.font_paths.push(assets_fonts);
        }
        font
    }

    fn typst_features(&self) -> Vec<Feature> {
        let mut features = self.args.compile.features.clone();
        for required in [Feature::Html, Feature::Bundle] {
            if !features.contains(&required) {
                features.push(required);
            }
        }
        features
    }

    fn base(&self) -> String {
        format!(
            "{}-{}-{}",
            self.package_info.namespace, self.package_info.name, self.package_info.version
        )
    }

    fn work_root(&self) -> PathBuf {
        self.source_root
            .parent()
            .expect("docs source root must have a parent")
            .to_owned()
    }

    fn stage_package(&self) -> Result<()> {
        remove_path_if_exists(&self.staging_root)?;
        if let Some(package_path) = &self.args.compile.package.package_path {
            let package_path = absolutize(package_path)?;
            if package_path != self.staging_root {
                copy_dir_all(&package_path, &self.staging_root)
                    .context_ut("failed to stage package path")?;
            }
        }

        let package_dir = self
            .staging_root
            .join(self.package_info.namespace.as_str())
            .join(self.package_info.name.as_str())
            .join(&self.package_info.version);
        if let Some(parent) = package_dir.parent() {
            std::fs::create_dir_all(parent)
                .context_ut("failed to create staged package parent directory")?;
        }
        remove_path_if_exists(&package_dir)?;
        copy_dir_all(&self.package_root, &package_dir).context_ut("failed to stage package")?;
        Ok(())
    }
}

async fn generate_and_export_docs(ctx: &PackageDocsContext, analysis: Arc<Analysis>) -> Result<()> {
    ctx.stage_package()?;
    let graph = docs_query_graph(ctx)?;
    let docs = analysis
        .clone()
        .query_snapshot(graph.clone(), None)
        .run_within_package(&ctx.package_info, |a| {
            let docs = tinymist_query::docs::package_docs(a, &ctx.package_info)
                .map_err(map_string_err("failed to generate package docs"))?;
            Ok(docs)
        })?;
    let public_api = docs.scip_public_api();

    let scip = analysis
        .query_snapshot(graph, None)
        .run_within_package(&ctx.package_info, |a| {
            let knowledge = tinymist_query::index::knowledge(a)
                .map_err(map_string_err("failed to generate package index"))?;
            knowledge
                .bind(a.shared())
                .to_scip_bytes_with_public_api(&public_api)
        })?;

    write_docs_inputs(ctx, &docs, scip)?;
    export_docs_bundle(ctx).await
}

fn docs_query_graph(
    ctx: &PackageDocsContext,
) -> Result<Arc<WorldComputeGraph<tinymist::project::LspCompilerFeat>>> {
    let query_path = ctx.work_root().join("query.typ");
    if let Some(parent) = query_path.parent() {
        std::fs::create_dir_all(parent)
            .context_ut("failed to create package docs query directory")?;
    }
    std::fs::write(&query_path, "").context_ut("failed to write package docs query input")?;
    let verse = ctx.query_compile_args().resolve()?;
    Ok(verse.computation())
}

fn write_docs_inputs(
    ctx: &PackageDocsContext,
    docs: &tinymist_query::docs::PackageDoc,
    scip: Vec<u8>,
) -> Result<()> {
    let base = ctx.base();
    let work_root = ctx
        .source_root
        .parent()
        .context("docs source root must have a parent")?;
    std::fs::create_dir_all(work_root)
        .context_ut("failed to create package docs work directory")?;
    std::fs::create_dir_all(&ctx.source_root)
        .context_ut("failed to create package docs source directory")?;

    std::fs::write(
        work_root.join(format!("{base}.json")),
        serde_json::to_vec_pretty(docs).context("failed to serialize package docs json")?,
    )
    .context("failed to write package docs json")?;
    std::fs::write(work_root.join(format!("{base}.scip")), scip)
        .context("failed to write package docs scip")?;

    let bundle = tinymist_query::docs::package_docs_bundle_typ(docs)
        .map_err(map_string_err("failed to generate package docs typ"))?;
    remove_path_if_exists(&ctx.source_root)?;
    for file in bundle {
        let path = ctx.source_root.join(file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .context_ut("failed to create package docs source parent directory")?;
        }
        std::fs::write(&path, file.content).context_ut("failed to write package docs source")?;
    }

    Ok(())
}

async fn export_docs_bundle(ctx: &PackageDocsContext) -> Result<()> {
    let verse = ctx.docs_compile_args().resolve()?;
    let graph = WorldComputeGraph::from_world(verse.snapshot());
    let artifact = CompiledArtifact::from_graph_without_doc(graph);
    if artifact.has_errors() {
        bail!("generated package docs failed to compile");
    }

    let task = ProjectTask::ExportBundle(ExportBundleTask {
        export: tinymist::project::ExportTask {
            when: TaskWhen::Never,
            output: Some(PathPattern::new(&ctx.dist.to_string_lossy())),
            transform: vec![],
        },
        pages: None,
        pdf_standards: ctx.args.compile.pdf.standard.clone(),
        no_pdf_tags: ctx.args.compile.pdf.no_tags,
        creation_timestamp: ctx.args.compile.creation_timestamp,
        ppi: Scalar::try_from(ctx.args.compile.png.ppi).context("cannot convert ppi")?,
    });
    ExportTask::do_export(task, artifact, None).await?;
    Ok(())
}

fn resolve_package_docs_input(args: &PackageDocsArgs, input: &str) -> Result<PackageDocsInput> {
    if !input.starts_with('@') {
        return Ok(PackageDocsInput {
            package_root: absolutize(Path::new(input))?,
            namespace: None,
        });
    }

    let spec =
        PackageSpec::from_str(input).map_err(map_string_err("failed to parse package spec"))?;
    let registry = tinymist::world::system::SystemUniverseBuilder::resolve_package(
        args.compile.cert.as_deref().map(From::from),
        Some(&args.compile.package),
    );
    let package_root = registry
        .resolve(&spec)
        .map_err(|err| anyhow::anyhow!("failed to resolve package {input}: {err}"))?;

    Ok(PackageDocsInput {
        package_root: package_root.as_ref().to_path_buf(),
        namespace: Some(spec.namespace.to_string()),
    })
}

fn find_repo_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().context("failed to get current directory")?;
    loop {
        if dir.join("typ/packages/package-docs/lib.typ").is_file()
            && dir.join("typ/packages/tinymist-index/lib.typ").is_file()
        {
            return Ok(dir);
        }
        if !dir.pop() {
            bail!("failed to find repository root containing typ/packages/package-docs/lib.typ");
        }
    }
}

fn absolutize(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .context("failed to get current directory")?
            .join(path))
    }
}

fn infer_namespace(package_root: &Path, manifest: &PackageManifest) -> Option<String> {
    let version = package_root.file_name()?.to_string_lossy();
    if version != manifest.package.version.to_string() {
        return None;
    }
    let name_dir = package_root.parent()?;
    if name_dir.file_name()?.to_string_lossy() != manifest.package.name.as_str() {
        return None;
    }
    Some(
        name_dir
            .parent()?
            .file_name()?
            .to_string_lossy()
            .into_owned(),
    )
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    if meta.is_dir() && !meta.file_type().is_symlink() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
    .context_ut("failed to remove path")
}
