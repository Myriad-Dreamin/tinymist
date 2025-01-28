use std::{path::Path, sync::OnceLock};

use clap::ValueHint;
use tinymist_std::{bail, error::prelude::Result};

pub use tinymist_world::args::{CompileFontArgs, CompilePackageArgs};
pub use typst_preview::{PreviewArgs, PreviewMode};

use crate::model::*;
use crate::PROJECT_ROUTE_USER_ACTION_PRIORITY;

/// Project document commands.
#[derive(Debug, Clone, clap::Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum DocCommands {
    /// Declare a document (project input).
    New(DocNewArgs),
    /// Configure document priority in workspace.
    Configure(DocConfigureArgs),
}

/// Project task commands.
#[derive(Debug, Clone, clap::Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum TaskCommands {
    /// Declare a preview task.
    Preview(TaskPreviewArgs),
}

/// Declare a document (project's input).
#[derive(Debug, Clone, clap::Parser)]
pub struct DocNewArgs {
    /// Argument to identify a project.
    #[clap(flatten)]
    pub id: DocIdArgs,
    /// Configures the project root (for absolute paths).
    #[clap(long = "root", env = "TYPST_ROOT", value_name = "DIR")]
    pub root: Option<String>,
    /// Common font arguments.
    #[clap(flatten)]
    pub font: CompileFontArgs,
    /// Common package arguments.
    #[clap(flatten)]
    pub package: CompilePackageArgs,
}

impl DocNewArgs {
    /// Converts to project input.
    pub fn to_input(&self) -> ProjectInput {
        let id: Id = (&self.id).into();

        let root = self
            .root
            .as_ref()
            .map(|root| ResourcePath::from_user_sys(Path::new(root)));
        let main = ResourcePath::from_user_sys(Path::new(&self.id.input));

        let font_paths = self
            .font
            .font_paths
            .iter()
            .map(|p| ResourcePath::from_user_sys(p))
            .collect::<Vec<_>>();

        let package_path = self
            .package
            .package_path
            .as_ref()
            .map(|p| ResourcePath::from_user_sys(p));

        let package_cache_path = self
            .package
            .package_cache_path
            .as_ref()
            .map(|p| ResourcePath::from_user_sys(p));

        ProjectInput {
            id: id.clone(),
            root,
            main,
            // todo: inputs
            inputs: vec![],
            font_paths,
            system_fonts: !self.font.ignore_system_fonts,
            package_path,
            package_cache_path,
        }
    }
}

/// The id of a document.
///
/// If an identifier is not provided, the document's path is used as the id.
#[derive(Debug, Clone, clap::Parser)]
pub struct DocIdArgs {
    /// Give a name to the document.
    #[clap(long = "name")]
    pub name: Option<String>,
    /// Path to input Typst file.
    #[clap(value_hint = ValueHint::FilePath)]
    pub input: String,
}

impl From<&ResourcePath> for Id {
    fn from(value: &ResourcePath) -> Self {
        Id::new(value.to_string())
    }
}

impl From<&DocIdArgs> for Id {
    fn from(args: &DocIdArgs) -> Self {
        if let Some(id) = &args.name {
            Id::new(id.clone())
        } else {
            (&ResourcePath::from_user_sys(Path::new(&args.input))).into()
        }
    }
}

/// Configure project's priorities.
#[derive(Debug, Clone, clap::Parser)]
pub struct DocConfigureArgs {
    /// Argument to identify a project.
    #[clap(flatten)]
    pub id: DocIdArgs,
    /// Set the unsigned priority of these task (lower numbers are higher
    /// priority).
    #[clap(long = "priority", default_value_t = PROJECT_ROUTE_USER_ACTION_PRIORITY)]
    pub priority: u32,
}

/// Declare an compile task.
#[derive(Debug, Clone, clap::Parser)]
pub struct TaskCompileArgs {
    /// Argument to identify a project.
    #[clap(flatten)]
    pub declare: DocNewArgs,

    /// Name a task.
    #[clap(long = "task")]
    pub task_name: Option<String>,

    /// When to run the task
    #[arg(long = "when")]
    pub when: Option<TaskWhen>,

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

    /// The output format.
    #[clap(skip)]
    pub output_format: OnceLock<Result<OutputFormat>>,
}

impl TaskCompileArgs {
    /// Convert the arguments to a project task.
    pub fn to_task(self, doc_id: Id) -> Result<ApplyProjectTask> {
        let new_task_id = self.task_name.map(Id::new);
        let task_id = new_task_id.unwrap_or(doc_id.clone());

        let output_format = if let Some(specified) = self.format {
            specified
        } else if let Some(output) = &self.output {
            let output = Path::new(output);

            match output.extension() {
                Some(ext) if ext.eq_ignore_ascii_case("pdf") => OutputFormat::Pdf,
                Some(ext) if ext.eq_ignore_ascii_case("png") => OutputFormat::Png,
                Some(ext) if ext.eq_ignore_ascii_case("svg") => OutputFormat::Svg,
                Some(ext) if ext.eq_ignore_ascii_case("html") => OutputFormat::Html,
                _ => bail!(
                    "could not infer output format for path {output:?}.\n\
                         consider providing the format manually with `--format/-f`",
                ),
            }
        } else {
            OutputFormat::Pdf
        };

        let when = self.when.unwrap_or(TaskWhen::Never);

        let mut transforms = vec![];

        if let Some(pages) = &self.pages {
            transforms.push(ExportTransform::Pages {
                ranges: pages.clone(),
            });
        }

        let export = ExportTask {
            when,
            output: None,
            transform: transforms,
        };

        let config = match output_format {
            OutputFormat::Pdf => ProjectTask::ExportPdf(ExportPdfTask {
                export,
                pdf_standards: self.pdf_standard.clone(),
                creation_timestamp: None,
            }),
            OutputFormat::Png => ProjectTask::ExportPng(ExportPngTask {
                export,
                ppi: self.ppi.try_into().unwrap(),
                fill: None,
            }),
            OutputFormat::Svg => ProjectTask::ExportSvg(ExportSvgTask { export }),
            OutputFormat::Html => ProjectTask::ExportSvg(ExportSvgTask { export }),
        };

        Ok(ApplyProjectTask {
            id: task_id.clone(),
            document: doc_id,
            task: config,
        })
    }
}

/// Declare an lsp task.
#[derive(Debug, Clone, clap::Parser)]
pub struct TaskPreviewArgs {
    /// Argument to identify a project.
    #[clap(flatten)]
    pub declare: DocNewArgs,

    /// Name a task.
    #[clap(long = "task")]
    pub name: Option<String>,

    /// When to run the task
    #[arg(long = "when")]
    pub when: Option<TaskWhen>,

    /// Preview arguments
    #[clap(flatten)]
    pub preview: PreviewArgs,

    /// Preview mode
    #[clap(long = "preview-mode", default_value = "document", value_name = "MODE")]
    pub preview_mode: PreviewMode,
}
