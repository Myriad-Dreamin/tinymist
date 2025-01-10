//! Project management tools.

use std::path::Path;

use anyhow::bail;
use clap::ValueHint;
use tinymist_project::*;
use typst_preview::{PreviewArgs, PreviewMode};

use crate::{CompileFontArgs, CompilePackageArgs};

trait LockFileExt {
    fn preview(&mut self, doc_id: Id, args: &TaskPreviewArgs) -> anyhow::Result<Id>;
    fn declare(&mut self, args: &DocNewArgs) -> Id;
    fn export(&mut self, doc_id: Id, args: &TaskCompileArgs) -> anyhow::Result<Id>;
}

impl LockFileExt for LockFile {
    fn declare(&mut self, args: &DocNewArgs) -> Id {
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

        self.replace_document(input);

        id
    }

    fn export(&mut self, doc_id: Id, args: &TaskCompileArgs) -> anyhow::Result<Id> {
        let task_id = args
            .task_name
            .as_ref()
            .map(|t| Id::new(t.clone()))
            .unwrap_or(doc_id.clone());

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

        let when = args.when.unwrap_or(TaskWhen::Never);

        let mut transforms = vec![];

        if let Some(pages) = &args.pages {
            transforms.push(ExportTransform::Pages(pages.clone()));
        }

        let export = ExportTask {
            document: doc_id,
            id: task_id.clone(),
            when,
            transform: transforms,
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

        self.replace_task(task);

        Ok(task_id)
    }

    fn preview(&mut self, doc_id: Id, args: &TaskPreviewArgs) -> anyhow::Result<Id> {
        let task_id = args
            .name
            .as_ref()
            .map(|t| Id::new(t.clone()))
            .unwrap_or(doc_id.clone());

        let when = args.when.unwrap_or(TaskWhen::OnType);
        let task = ProjectTask::Preview(PreviewTask {
            id: task_id.clone(),
            doc_id,
            when,
        });

        self.replace_task(task);

        Ok(task_id)
    }
}

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
    /// Declare a compile task (output).
    Compile(TaskCompileArgs),
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

/// Configure project's priorities.
#[derive(Debug, Clone, clap::Parser)]
pub struct DocConfigureArgs {
    /// Argument to identify a project.
    #[clap(flatten)]
    pub id: DocIdArgs,
    /// Set the unsigned priority of these task (lower numbers are higher
    /// priority).
    #[clap(long = "priority", default_value_t = 0)]
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

/// Project document commands' main
pub fn project_main(args: DocCommands) -> anyhow::Result<()> {
    LockFile::update(Path::new("."), |state| {
        match args {
            DocCommands::New(args) => {
                state.declare(&args);
            }
            DocCommands::Configure(args) => {
                let id: Id = (&args.id).into();

                state.route.push(ProjectRoute {
                    id: id.clone(),
                    priority: args.priority,
                });
            }
        }

        Ok(())
    })
}

/// Project task commands' main
pub fn task_main(args: TaskCommands) -> anyhow::Result<()> {
    LockFile::update(Path::new("."), |state| {
        match args {
            TaskCommands::Compile(args) => {
                let id = state.declare(&args.declare);
                let _ = state.export(id, &args);
            }
            TaskCommands::Preview(args) => {
                let id = state.declare(&args.declare);
                let _ = state.preview(id, &args);
            }
        }

        Ok(())
    })
}
