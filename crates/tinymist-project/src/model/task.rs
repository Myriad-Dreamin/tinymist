//! Project task models.

use std::hash::Hash;

use serde::{Deserialize, Serialize};

use super::{Id, Pages, PathPattern, PdfStandard, Scalar, TaskWhen};

/// A project task application specifier. This is used for specifying tasks to
/// run in a project. When the language service notifies an update event of the
/// project, it will check whether any associated tasks need to be run.
///
/// Each task can have different timing and conditions for running. See
/// [`TaskWhen`] for more information.
///
/// The available task types listed in the [`ProjectTask`] only represent the
/// direct formats supported by the typst compiler. More task types can be
/// customized by the [`ExportTransform`].
///
/// ## Examples
///
/// Export a JSON file with the pdfpc notes of the document:
///
/// ```bash
/// tinymist project query main.typ --format json --selector "<pdfpc-notes>" --field value --one
/// ```
///
/// Export a PDF file and then runs a ghostscript command to compress it:
///
/// ```bash
/// tinymist project compile main.typ --pipe 'import "@local/postprocess:0.0.1": ghostscript; ghostscript(output.path)'
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type")]
pub struct ApplyProjectTask {
    /// The task's ID.
    pub id: Id,
    /// The document's ID.
    pub document: Id,
    /// The task to run.
    #[serde(flatten)]
    pub task: ProjectTask,
}

impl ApplyProjectTask {
    /// Returns the document's ID.
    pub fn doc_id(&self) -> &Id {
        &self.document
    }

    /// Returns the task's ID.
    pub fn id(&self) -> &Id {
        &self.id
    }
}

/// A project task specifier. This structure specifies the arguments for a task.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type")]
pub enum ProjectTask {
    /// A preview task.
    Preview(PreviewTask),
    /// An export PDF task.
    ExportPdf(ExportPdfTask),
    /// An export PNG task.
    ExportPng(ExportPngTask),
    /// An export SVG task.
    ExportSvg(ExportSvgTask),
    /// An export HTML task.
    ExportHtml(ExportHtmlTask),
    /// An export Markdown task.
    ExportMarkdown(ExportMarkdownTask),
    /// An export Text task.
    ExportText(ExportTextTask),
    /// An query task.
    Query(QueryTask),
    // todo: compatibility
    // An export task of another type.
    // Other(serde_json::Value),
}

impl ProjectTask {
    /// Returns the timing of executing the task.
    pub fn when(&self) -> Option<TaskWhen> {
        Some(match self {
            Self::Preview(task) => task.when,
            Self::ExportPdf(..)
            | Self::ExportPng(..)
            | Self::ExportSvg(..)
            | Self::ExportHtml(..)
            | Self::ExportMarkdown(..)
            | Self::ExportText(..)
            | Self::Query(..) => self.as_export()?.when,
        })
    }

    /// Returns the export configuration of a task.
    pub fn as_export(&self) -> Option<&ExportTask> {
        Some(match self {
            Self::Preview(..) => return None,
            Self::ExportPdf(task) => &task.export,
            Self::ExportPng(task) => &task.export,
            Self::ExportSvg(task) => &task.export,
            Self::ExportHtml(task) => &task.export,
            Self::ExportMarkdown(task) => &task.export,
            Self::ExportText(task) => &task.export,
            Self::Query(task) => &task.export,
        })
    }

    /// Returns extension of the artifact.
    pub fn extension(&self) -> &str {
        match self {
            Self::ExportPdf { .. } => "pdf",
            Self::Preview(..) | Self::ExportHtml { .. } => "html",
            Self::ExportMarkdown { .. } => "md",
            Self::ExportText { .. } => "txt",
            Self::ExportSvg { .. } => "svg",
            Self::ExportPng { .. } => "png",
            Self::Query(QueryTask {
                format,
                output_extension,
                ..
            }) => output_extension.as_deref().unwrap_or(format),
        }
    }
}

/// A preview task specifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PreviewTask {
    /// When to run the task. See [`TaskWhen`] for more
    /// information.
    pub when: TaskWhen,
}

/// An export task specifier.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExportTask {
    /// When to run the task
    pub when: TaskWhen,
    /// The output path pattern.
    pub output: Option<PathPattern>,
    /// The task's transforms.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub transform: Vec<ExportTransform>,
}

impl ExportTask {
    /// Creates a new unmounted export task.
    pub fn new(when: TaskWhen) -> Self {
        Self {
            when,
            output: None,
            transform: Vec::new(),
        }
    }

    /// Pretty prints the output whenever possible.
    pub fn apply_pretty(&mut self) {
        self.transform
            .push(ExportTransform::Pretty { script: None });
    }
}

/// The legacy page selection specifier.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageSelection {
    /// Selects the first page.
    #[default]
    First,
    /// Merges all pages into a single page.
    Merged {
        /// The gap between pages (in pt).
        gap: Option<String>,
    },
}

/// A project export transform specifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExportTransform {
    /// Only pick a subset of pages.
    Pages {
        /// The page ranges to export.
        ranges: Vec<Pages>,
    },
    /// Merge pages into a single page.
    Merge {
        /// The gap between pages (typst code expression, e.g. `1pt`).
        gap: Option<String>,
    },
    /// Execute a transform script.
    Script {
        /// The postprocess script (typst script) to run.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        script: Option<String>,
    },
    /// Uses a pretty printer to format the output.
    Pretty {
        /// The pretty command (typst script) to run.
        ///
        /// If not provided, the default pretty printer will be used.
        /// Note: the builtin one may be only effective for json outputs.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        script: Option<String>,
    },
}

/// An export pdf task specifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExportPdfTask {
    /// The shared export arguments.
    #[serde(flatten)]
    pub export: ExportTask,
    /// One (or multiple comma-separated) PDF standards that Typst will enforce
    /// conformance with.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub pdf_standards: Vec<PdfStandard>,
    /// The document's creation date formatted as a UNIX timestamp (in seconds).
    ///
    /// For more information, see <https://reproducible-builds.org/specs/source-date-epoch/>.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub creation_timestamp: Option<i64>,
}

/// An export png task specifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExportPngTask {
    /// The shared export arguments.
    #[serde(flatten)]
    pub export: ExportTask,
    /// The PPI (pixels per inch) to use for PNG export.
    pub ppi: Scalar,
    /// The expression constructing background fill color (in typst script).
    /// e.g. `#ffffff`, `#000000`, `rgba(255, 255, 255, 0.5)`.
    ///
    /// If not provided, the default background color specified in the document
    /// will be used.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fill: Option<String>,
}

/// An export svg task specifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExportSvgTask {
    /// The shared export arguments.
    #[serde(flatten)]
    pub export: ExportTask,
}

/// An export html task specifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExportHtmlTask {
    /// The shared export arguments.
    #[serde(flatten)]
    pub export: ExportTask,
}

/// An export markdown task specifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExportMarkdownTask {
    /// The shared export arguments.
    #[serde(flatten)]
    pub export: ExportTask,
}

/// An export text task specifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ExportTextTask {
    /// The shared export arguments.
    #[serde(flatten)]
    pub export: ExportTask,
}

/// An export query task specifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct QueryTask {
    /// The shared export arguments.
    #[serde(flatten)]
    pub export: ExportTask,
    /// The format to serialize in. Can be `json`, `yaml`, or `txt`,
    pub format: String,
    /// Uses a different output extension from the one inferring from the
    /// [`Self::format`].
    pub output_extension: Option<String>,
    /// Defines which elements to retrieve.
    pub selector: String,
    /// Extracts just one field from all retrieved elements.
    pub field: Option<String>,
    /// Expects and retrieves exactly one element.
    pub one: bool,
}
