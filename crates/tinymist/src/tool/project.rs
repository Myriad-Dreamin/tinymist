//! Project management tools.

use core::fmt;
use std::{num::NonZeroUsize, ops::RangeInclusive, path::Path, str::FromStr};

use anyhow::bail;
use clap::{ValueEnum, ValueHint};
use reflexo::path::unix_slash;

use crate::{CompileFontArgs, CompilePackageArgs};

/// Project commands.
#[derive(Debug, Clone, clap::Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum ProjectCommands {
    /// Declare a project input.
    New(ProjectNewArgs),
    /// Configure project priority in workspace.
    Configure(ProjectConfigureArgs),
}

/// Project commands.
#[derive(Debug, Clone, clap::Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum TaskCommands {
    /// Declare a project task (output).
    New(TaskNewArgs),
}

/// Declare a project input.
#[derive(Debug, Clone, clap::Parser)]
pub struct ProjectIdArgs {
    /// Give an id (name) to project.
    #[clap(long = "name")]
    pub id: Option<String>,
    /// Path to input Typst file.
    #[clap(value_hint = ValueHint::FilePath)]
    pub input: String,
}

/// Declare a project input.
#[derive(Debug, Clone, clap::Parser)]
pub struct ProjectNewArgs {
    /// Argument to identify a project.
    #[clap(flatten)]
    pub id: ProjectIdArgs,
    /// Configures the project root (for absolute paths).
    #[clap(long = "root", env = "TYPST_ROOT", value_name = "DIR")]
    pub root: Option<String>,
    /// Common font arguments.
    #[clap(flatten)]
    pub font: CompileFontArgs,
    /// Common font arguments.
    #[clap(flatten)]
    pub package: CompilePackageArgs,
}

/// Configure project's priorities.
#[derive(Debug, Clone, clap::Parser)]
pub struct ProjectConfigureArgs {
    /// Argument to identify a project.
    #[clap(flatten)]
    pub id: ProjectIdArgs,
    /// Set the unsigned priority of these task (lower numbers are higher
    /// priority).
    #[clap(long = "priority", default_value_t = 0)]
    pub priority: u32,
}

/// Declare an export task.
#[derive(Debug, Clone, clap::Parser)]
pub struct TaskNewArgs {
    /// Argument to identify a project.
    #[clap(flatten)]
    pub declare: ProjectNewArgs,

    /// Name a task.
    #[clap(long = "task")]
    pub task: Option<String>,

    /// Path to output file (PDF, PNG, SVG, or HTML). Use `-` to write output to
    /// stdout.
    ///
    /// For output formats emitting one file per page (PNG & SVG), a page number
    /// template must be present if the source document renders to multiple
    /// pages. Use `{p}` for page numbers, `{0p}` for zero padded page numbers
    /// and `{t}` for page count. For example, `page-{0p}-of-{t}.png` creates
    /// `page-01-of-10.png`, `page-02-of-10.png`, and so on.
    #[clap(value_hint = ValueHint::FilePath)]
    pub output: Option<String>,

    /// The format of the output file, inferred from the extension by default.
    #[arg(long = "format", short = 'f')]
    pub format: Option<OutputFormat>,

    /// Which pages to export. When unspecified, all pages are exported.
    ///
    /// Pages to export are separated by commas, and can be either simple page
    /// numbers (e.g. '2,5' to export only pages 2 and 5) or page ranges (e.g.
    /// '2,3-6,8-' to export page 2, pages 3 to 6 (inclusive), page 8 and any
    /// pages after it).
    ///
    /// Page numbers are one-indexed and correspond to physical page numbers in
    /// the document (therefore not being affected by the document's page
    /// counter).
    #[arg(long = "pages", value_delimiter = ',')]
    pub pages: Option<Vec<Pages>>,

    /// One (or multiple comma-separated) PDF standards that Typst will enforce
    /// conformance with.
    #[arg(long = "pdf-standard", value_delimiter = ',')]
    pub pdf_standard: Vec<PdfStandard>,

    /// The PPI (pixels per inch) to use for PNG export.
    #[arg(long = "ppi", default_value_t = 144.0)]
    pub ppi: f32,

    /// When to run the task
    #[arg(long = "when")]
    pub when: Option<ExportWhen>,
}

/// Implements parsing of page ranges (`1-3`, `4`, `5-`, `-2`), used by the
/// `CompileCommand.pages` argument, through the `FromStr` trait instead of a
/// value parser, in order to generate better errors.
///
/// See also: https://github.com/clap-rs/clap/issues/5065
#[derive(Debug, Clone)]
pub struct Pages(pub RangeInclusive<Option<NonZeroUsize>>);

impl FromStr for Pages {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value
            .split('-')
            .map(str::trim)
            .collect::<Vec<_>>()
            .as_slice()
        {
            [] | [""] => Err("page export range must not be empty"),
            [single_page] => {
                let page_number = parse_page_number(single_page)?;
                Ok(Pages(Some(page_number)..=Some(page_number)))
            }
            ["", ""] => Err("page export range must have start or end"),
            [start, ""] => Ok(Pages(Some(parse_page_number(start)?)..=None)),
            ["", end] => Ok(Pages(None..=Some(parse_page_number(end)?))),
            [start, end] => {
                let start = parse_page_number(start)?;
                let end = parse_page_number(end)?;
                if start > end {
                    Err("page export range must end at a page after the start")
                } else {
                    Ok(Pages(Some(start)..=Some(end)))
                }
            }
            [_, _, _, ..] => Err("page export range must have a single hyphen"),
        }
    }
}

impl fmt::Display for Pages {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start = match self.0.start() {
            Some(start) => start.to_string(),
            None => String::from(""),
        };
        let end = match self.0.end() {
            Some(end) => end.to_string(),
            None => String::from(""),
        };
        write!(f, "{start}-{end}")
    }
}

impl serde::Serialize for Pages {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for Pages {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

/// Parses a single page number.
fn parse_page_number(value: &str) -> Result<NonZeroUsize, &'static str> {
    if value == "0" {
        Err("page numbers start at one")
    } else {
        NonZeroUsize::from_str(value).map_err(|_| "not a valid page number")
    }
}

macro_rules! display_possible_values {
    ($ty:ty) => {
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.to_possible_value()
                    .expect("no values are skipped")
                    .get_name()
                    .fmt(f)
            }
        }
    };
}

/// When to export an output file.
#[derive(
    Debug,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    serde::Serialize,
    serde::Deserialize,
    ValueEnum,
)]
#[serde(rename_all = "camelCase")]
#[clap(rename_all = "camelCase")]
pub enum ExportWhen {
    /// Never export.
    Never,
    /// Export on save.
    OnSave,
    /// Export on type.
    OnType,
}

impl ExportWhen {
    /// Returns `true` if the task should never be run automatically.
    pub fn is_never(&self) -> bool {
        matches!(self, ExportWhen::Never)
    }
}

display_possible_values!(ExportWhen);

/// Which format to use for the generated output file.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, ValueEnum)]
pub enum OutputFormat {
    /// Export to PDF.
    Pdf,
    /// Export to PNG.
    Png,
    /// Export to SVG.
    Svg,
    /// Export to HTML.
    Html,
}

display_possible_values!(OutputFormat);

/// A PDF standard that Typst can enforce conformance with.
#[derive(Debug, Copy, Clone, Eq, PartialEq, ValueEnum, serde::Serialize, serde::Deserialize)]
#[allow(non_camel_case_types)]
pub enum PdfStandard {
    /// PDF 1.7.
    #[value(name = "1.7")]
    #[serde(rename = "1.7")]
    V_1_7,
    /// PDF/A-2b.
    #[value(name = "a-2b")]
    #[serde(rename = "a-2b")]
    A_2b,
}

display_possible_values!(PdfStandard);

const LOCK_VERSION: &str = "0.1.0-beta0";

#[derive(serde::Serialize, serde::Deserialize)]
struct LockFile {
    /// The lock file version.
    version: String,
    /// The project's document (input).
    document: Vec<ProjectInput>,
    /// The project's task (output).
    task: Vec<ProjectTask>,
    /// The project's task route.
    route: Vec<ProjectRoute>,
}

impl LockFile {
    fn declare(&mut self, args: &ProjectNewArgs) -> Id {
        let id: Id = (&args.id).into();

        let root = args
            .root
            .as_ref()
            .map(|root| ResourcePath::from_user_sys(Path::new(root)));

        let font_paths = args
            .font
            .font_paths
            .iter()
            .map(|p| ResourcePath::from_user_sys(p))
            .collect::<Vec<_>>();

        let package_path = args
            .package
            .package_path
            .as_ref()
            .map(|p| ResourcePath::from_user_sys(p));

        let package_cache_path = args
            .package
            .package_cache_path
            .as_ref()
            .map(|p| ResourcePath::from_user_sys(p));

        let input = ProjectInput {
            id: id.clone(),
            root,
            font_paths,
            system_fonts: !args.font.ignore_system_fonts,
            package_path,
            package_cache_path,
        };

        self.document.push(input);

        id
    }

    fn export(&mut self, id: Id, args: &TaskNewArgs) -> anyhow::Result<Id> {
        let task_id = args
            .task
            .as_ref()
            .map(|t| Id(t.clone()))
            .unwrap_or(id.clone());

        let output_format = if let Some(specified) = args.format {
            specified
        } else if let Some(output) = &args.output {
            let output = Path::new(output);

            match output.extension() {
                Some(ext) if ext.eq_ignore_ascii_case("pdf") => OutputFormat::Pdf,
                Some(ext) if ext.eq_ignore_ascii_case("png") => OutputFormat::Png,
                Some(ext) if ext.eq_ignore_ascii_case("svg") => OutputFormat::Svg,
                Some(ext) if ext.eq_ignore_ascii_case("html") => OutputFormat::Html,
                _ => bail!(
                    "could not infer output format for path {}.\n\
                         consider providing the format manually with `--format/-f`",
                    output.display()
                ),
            }
        } else {
            OutputFormat::Pdf
        };

        let when = args.when.unwrap_or(ExportWhen::Never);

        let export = ExportTask {
            id,
            when,
            transforms: vec![],
        };

        let task = match output_format {
            OutputFormat::Pdf => ProjectTask::ExportPdf(ExportPdfTask {
                export,
                pdf_standards: args.pdf_standard.clone(),
            }),
            OutputFormat::Png => ProjectTask::ExportPng(ExportPngTask {
                export,
                ppi: args.ppi,
            }),
            OutputFormat::Svg => ProjectTask::ExportSvg(ExportSvgTask { export }),
            OutputFormat::Html => ProjectTask::ExportSvg(ExportSvgTask { export }),
        };

        self.task.push(task);

        Ok(task_id)
    }
}

fn serialize_resolve(resolve: &LockFile) -> String {
    let content = toml::Table::try_from(resolve).unwrap();

    let mut out = String::new();

    // At the start of the file we notify the reader that the file is generated.
    // Specifically Phabricator ignores files containing "@generated", so we use
    // that.
    let marker_line = "# This file is automatically @generated by tinymist.";
    let extra_line = "# It is not intended for manual editing.";

    out.push_str(marker_line);
    out.push('\n');
    out.push_str(extra_line);
    out.push('\n');

    if let Some(version) = content.get("version") {
        out.push_str(&format!("version = {version}\n"));
    }

    let document = content["document"].as_array().unwrap();
    for document in document {
        out.push('\n');
        out.push_str("[[document]]\n");
        emit_document(document, &mut out);
    }

    let task = content["task"].as_array().unwrap();
    for task in task {
        out.push('\n');
        out.push_str("[[task]]\n");
        emit_output(task, &mut out);
    }

    let route = content["route"].as_array().unwrap();
    for route in route {
        out.push('\n');
        out.push_str("[[route]]\n");
        emit_route(route, &mut out);
    }

    return out;

    fn emit_document(input: &toml::Value, out: &mut String) {
        let table = input.as_table().unwrap();
        out.push_str(&table.to_string());
    }

    fn emit_output(output: &toml::Value, out: &mut String) {
        let table = output.as_table().unwrap();
        out.push_str(&table.to_string());
    }

    fn emit_route(route: &toml::Value, out: &mut String) {
        let table = route.as_table().unwrap();
        out.push_str(&table.to_string());
    }
}

/// A project ID.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Id(String);

impl From<&ProjectIdArgs> for Id {
    fn from(args: &ProjectIdArgs) -> Self {
        if let Some(id) = &args.id {
            Id(id.clone())
        } else {
            let inp = Path::new(&args.input);
            Id(ResourcePath::from_user_sys(inp).to_string())
        }
    }
}

/// A resource path.
#[derive(Debug, Clone)]
pub struct ResourcePath(String, String);

impl fmt::Display for ResourcePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.0, self.1)
    }
}

impl FromStr for ResourcePath {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut parts = value.split(':');
        let scheme = parts.next().ok_or("missing scheme")?;
        let path = parts.next().ok_or("missing path")?;
        if parts.next().is_some() {
            Err("too many colons")
        } else {
            Ok(ResourcePath(scheme.to_string(), path.to_string()))
        }
    }
}

impl serde::Serialize for ResourcePath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for ResourcePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

impl ResourcePath {
    fn from_user_sys(inp: &Path) -> Self {
        let rel = if inp.is_relative() {
            inp.to_path_buf()
        } else {
            let cwd = std::env::current_dir().unwrap();
            pathdiff::diff_paths(inp, &cwd).unwrap()
        };
        let rel = unix_slash(&rel);
        ResourcePath("file".to_string(), rel.to_string())
    }
}

/// A project input specifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectInput {
    /// The project's ID.
    pub id: Id,
    /// The project's root directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<ResourcePath>,
    /// The project's font paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub font_paths: Vec<ResourcePath>,
    /// Whether to use system fonts.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub system_fonts: bool,
    /// The project's package path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_path: Option<ResourcePath>,
    /// The project's package cache path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_cache_path: Option<ResourcePath>,
}

/// A project export transform specifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportTransform {
    /// Only pick a subset of pages.
    Pages(Vec<Pages>),
}

/// A project task specifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectTask {
    /// A lsp task.
    Lsp,
    /// An export PDF task.
    ExportPdf(ExportPdfTask),
    /// An export PNG task.
    ExportPng(ExportPngTask),
    /// An export SVG task.
    ExportSvg(ExportSvgTask),
    /// An export task of another type.
    Other(serde_json::Value),
}

/// A project route specifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectRoute {
    /// A project.
    id: Id,
    /// The priority of the project.
    priority: u32,
}

/// An export task specifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExportTask {
    /// The task's ID.
    pub id: Id,
    /// When to run the task
    pub when: ExportWhen,
    /// The task's transforms.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub transforms: Vec<ExportTransform>,
}

/// An export pdf task specifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExportPdfTask {
    /// The shared export arguments
    #[serde(flatten)]
    pub export: ExportTask,
    /// The pdf standards.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pdf_standards: Vec<PdfStandard>,
}

/// An export png task specifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExportPngTask {
    /// The shared export arguments
    #[serde(flatten)]
    pub export: ExportTask,
    /// The PPI (pixels per inch) to use for PNG export.
    pub ppi: f32,
}

/// An export png task specifier.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExportSvgTask {
    /// The shared export arguments
    #[serde(flatten)]
    pub export: ExportTask,
}

/// Project commands' main
pub fn project_main(args: ProjectCommands) -> anyhow::Result<()> {
    let mut state = LockFile {
        version: LOCK_VERSION.to_string(),
        document: vec![],
        task: vec![],
        route: vec![],
    };

    match args {
        ProjectCommands::New(args) => {
            state.declare(&args);
        }
        ProjectCommands::Configure(args) => {
            let id: Id = (&args.id).into();

            state.route.push(ProjectRoute {
                id: id.clone(),
                priority: args.priority,
            });
        }
    }

    let content = serialize_resolve(&state);
    std::fs::write("tinymist.lock", content).unwrap();

    Ok(())
}

/// Project task commands' main
pub fn task_main(args: TaskCommands) -> anyhow::Result<()> {
    let mut state = LockFile {
        version: LOCK_VERSION.to_string(),
        document: vec![],
        task: vec![],
        route: vec![],
    };

    match args {
        TaskCommands::New(args) => {
            let id = state.declare(&args.declare);
            let _ = state.export(id, &args);
        }
    }

    let content = serialize_resolve(&state);
    std::fs::write("tinymist.lock", content).unwrap();

    Ok(())
}
