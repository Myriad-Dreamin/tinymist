use std::{path::Path, sync::OnceLock};

use clap::ValueHint;
use tinymist_std::{bail, error::prelude::Result};

use tinymist_world::args::PdfExportArgs;
use tinymist_world::args::PngExportArgs;
pub use tinymist_world::args::{CompileFontArgs, CompilePackageArgs};

use crate::PROJECT_ROUTE_USER_ACTION_PRIORITY;
use crate::model::*;

/// Project document commands.
#[derive(Debug, Clone, clap::Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum DocCommands {
    /// Declare a document (project input).
    New(DocNewArgs),
    /// Configure document priority in workspace.
    Configure(DocConfigureArgs),
}

/// Declare a document (project's input).
#[derive(Debug, Clone, clap::Parser)]
pub struct DocNewArgs {
    /// Specify the argument to identify a project.
    #[clap(flatten)]
    pub id: DocIdArgs,
    /// Configure the project root (for absolute paths). If the path is
    /// relative, it will be resolved relative to the current working directory
    /// (PWD).
    #[clap(long = "root", env = "TYPST_ROOT", value_name = "DIR")]
    pub root: Option<String>,
    /// Specify the font related arguments.
    #[clap(flatten)]
    pub font: CompileFontArgs,
    /// Specify the package related arguments.
    #[clap(flatten)]
    pub package: CompilePackageArgs,
}

impl DocNewArgs {
    /// Converts to project input.
    pub fn to_input(&self, ctx: CtxPath) -> ProjectInput {
        let id: Id = self.id.id(ctx);

        let root = self
            .root
            .as_ref()
            .map(|root| ResourcePath::from_user_sys(Path::new(root), ctx));
        let main = ResourcePath::from_user_sys(Path::new(&self.id.input), ctx);

        let font_paths = self
            .font
            .font_paths
            .iter()
            .map(|p| ResourcePath::from_user_sys(p, ctx))
            .collect::<Vec<_>>();

        let package_path = self
            .package
            .package_path
            .as_ref()
            .map(|p| ResourcePath::from_user_sys(p, ctx));

        let package_cache_path = self
            .package
            .package_cache_path
            .as_ref()
            .map(|p| ResourcePath::from_user_sys(p, ctx));

        ProjectInput {
            id: id.clone(),
            lock_dir: Some(ctx.1.to_path_buf()),
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

/// Specify the id of a document.
///
/// If an identifier is not provided, the document's path is used as the id.
#[derive(Debug, Clone, clap::Parser)]
pub struct DocIdArgs {
    /// Give a task name to the document.
    #[clap(long = "name")]
    pub name: Option<String>,
    /// Specify the path to input Typst file.
    #[clap(value_hint = ValueHint::FilePath)]
    pub input: String,
}

impl DocIdArgs {
    /// Converts to a document ID.
    pub fn id(&self, ctx: CtxPath) -> Id {
        if let Some(id) = &self.name {
            Id::new(id.clone())
        } else {
            (&ResourcePath::from_user_sys(Path::new(&self.input), ctx)).into()
        }
    }
}

/// Configure project's priorities.
#[derive(Debug, Clone, clap::Parser)]
pub struct DocConfigureArgs {
    /// Specify the argument to identify a project.
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
    /// Specify the argument to identify a project.
    #[clap(flatten)]
    pub declare: DocNewArgs,

    /// Configure when to run the task.
    #[arg(long = "when")]
    pub when: Option<TaskWhen>,

    /// Provide the path to output file (PDF, PNG, SVG, or HTML). Use `-` to
    /// write output to stdout.
    ///
    /// For output formats emitting one file per page (PNG & SVG), a page number
    /// template must be present if the source document renders to multiple
    /// pages. Use `{p}` for page numbers, `{0p}` for zero padded page numbers
    /// and `{t}` for page count. For example, `page-{0p}-of-{t}.png` creates
    /// `page-01-of-10.png`, `page-02-of-10.png`, and so on.
    #[clap(value_hint = ValueHint::FilePath)]
    pub output: Option<String>,

    /// Specify the format of the output file, inferred from the extension by
    /// default.
    #[arg(long = "format", short = 'f')]
    pub format: Option<OutputFormat>,

    /// Specify which pages to export. When unspecified, all pages are exported.
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

    /// Specify the PDF export related arguments.
    #[clap(flatten)]
    pub pdf: PdfExportArgs,

    /// Specify the PNG export related arguments.
    #[clap(flatten)]
    pub png: PngExportArgs,

    /// Specify the output format.
    #[clap(skip)]
    pub output_format: OnceLock<Result<OutputFormat>>,
}

impl TaskCompileArgs {
    /// Converts the arguments to a project task.
    pub fn to_task(self, doc_id: Id, cwd: &Path) -> Result<ApplyProjectTask> {
        let new_task_id = self.declare.id.name.map(Id::new);
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

        let output = self.output.as_ref().map(|output| {
            let output = Path::new(output);
            let output = if output.is_absolute() {
                output.to_path_buf()
            } else {
                cwd.join(output)
            };

            PathPattern::new(&output.with_extension("").to_string_lossy())
        });

        let when = self.when.unwrap_or(TaskWhen::Never);

        let mut transforms = vec![];

        if let Some(pages) = &self.pages {
            transforms.push(ExportTransform::Pages {
                ranges: pages.clone(),
            });
        }

        let export = ExportTask {
            when,
            output,
            transform: transforms,
        };

        let config = match output_format {
            OutputFormat::Pdf => ProjectTask::ExportPdf(ExportPdfTask {
                export,
                pages: self.pages.clone(),
                pdf_standards: self.pdf.standard.clone(),
                no_pdf_tags: self.pdf.no_tags,
                creation_timestamp: None,
            }),
            OutputFormat::Png => ProjectTask::ExportPng(ExportPngTask {
                export,
                pages: self.pages.clone(),
                page_number_template: None,
                merge: None,
                ppi: self.png.ppi.try_into().unwrap(),
                fill: None,
            }),
            OutputFormat::Svg => ProjectTask::ExportSvg(ExportSvgTask {
                export,
                pages: self.pages.clone(),
                page_number_template: None,
                merge: None,
            }),
            OutputFormat::Html => ProjectTask::ExportHtml(ExportHtmlTask { export }),
        };

        Ok(ApplyProjectTask {
            id: task_id.clone(),
            document: doc_id,
            task: config,
        })
    }
}
