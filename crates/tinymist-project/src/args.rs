use crate::DocIdArgs;
use core::fmt;
use serde::{Deserialize, Serialize};
use std::{num::NonZeroUsize, ops::RangeInclusive, path::Path, str::FromStr, sync::OnceLock};
use tinymist_std::{bail, error::prelude::ZResult};
pub use tinymist_world::args::{CompileFontArgs, CompilePackageArgs};
pub use typst_preview::{PreviewArgs, PreviewMode};

use anyhow::Result;
use clap::{ValueEnum, ValueHint};

use crate::model::*;
use crate::PROJECT_ROUTE_USER_ACTION_PRIORITY;

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
    Debug, Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd, ValueEnum, Serialize, Deserialize,
)]
#[serde(rename_all = "camelCase")]
#[clap(rename_all = "camelCase")]
pub enum TaskWhen {
    /// Never watch to run task.
    Never,
    /// Run task on save.
    OnSave,
    /// Run task on type.
    OnType,
    /// *DEPRECATED* Run task when a document has a title and on saved, which is
    /// useful to filter out template files.
    ///
    /// Note: this is deprecating.
    OnDocumentHasTitle,
}

impl TaskWhen {
    /// Returns `true` if the task should never be run automatically.
    pub fn is_never(&self) -> bool {
        matches!(self, TaskWhen::Never)
    }
}

display_possible_values!(TaskWhen);

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
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, ValueEnum, Serialize, Deserialize)]
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

/// Implements parsing of page ranges (`1-3`, `4`, `5-`, `-2`), used by the
/// `CompileCommand.pages` argument, through the `FromStr` trait instead of a
/// value parser, in order to generate better errors.
///
/// See also: <https://github.com/clap-rs/clap/issues/5065>
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
// clippy bug: TaskCompileArgs is evaluated as 0 bytes
#[allow(clippy::large_enum_variant)]
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

    #[clap(skip)]
    pub output_format: OnceLock<ZResult<OutputFormat>>,
}

impl TaskCompileArgs {
    pub fn to_task(self, doc_id: Id) -> ZResult<ProjectTask> {
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
            document: doc_id,
            id: task_id.clone(),
            when,
            transform: transforms,
        };

        Ok(match output_format {
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
