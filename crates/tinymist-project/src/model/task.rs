use std::hash::Hash;

pub use anyhow::Result;
use serde::{Deserialize, Serialize};
use tinymist_derive::toml_model;

use super::{Id, Pages, PdfStandard, Scalar, TaskWhen};

/// A project task specifier.
#[toml_model]
#[serde(tag = "type")]
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
    /// Returns the task's ID.
    pub fn doc_id(&self) -> &Id {
        match self {
            ProjectTask::Preview(task) => &task.document,
            ProjectTask::ExportPdf(task) => &task.export.document,
            ProjectTask::ExportPng(task) => &task.export.document,
            ProjectTask::ExportSvg(task) => &task.export.document,
            ProjectTask::ExportHtml(task) => &task.export.document,
            ProjectTask::ExportMarkdown(task) => &task.export.document,
            ProjectTask::ExportText(task) => &task.export.document,
            ProjectTask::Query(task) => &task.export.document,
            // ProjectTask::Other(_) => return None,
        }
    }

    /// Returns the task's ID.
    pub fn id(&self) -> &Id {
        match self {
            ProjectTask::Preview(task) => &task.id,
            ProjectTask::ExportPdf(task) => &task.export.id,
            ProjectTask::ExportPng(task) => &task.export.id,
            ProjectTask::ExportSvg(task) => &task.export.id,
            ProjectTask::ExportHtml(task) => &task.export.id,
            ProjectTask::ExportMarkdown(task) => &task.export.id,
            ProjectTask::ExportText(task) => &task.export.id,
            ProjectTask::Query(task) => &task.export.id,
            // ProjectTask::Other(_) => return None,
        }
    }
}

/// An lsp task specifier.
#[toml_model]
pub struct PreviewTask {
    /// The task's ID.
    pub id: Id,
    /// The doc's ID.
    pub document: Id,
    /// When to run the task
    pub when: TaskWhen,
}

/// An export task specifier.
#[toml_model]
pub struct ExportTask {
    /// The task's ID.
    pub id: Id,
    /// The doc's ID.
    pub document: Id,
    /// When to run the task
    pub when: TaskWhen,
    /// The task's transforms.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub transform: Vec<ExportTransform>,
}

/// A project export transform specifier.
#[toml_model]
pub enum ExportTransform {
    /// Only pick a subset of pages.
    Pages {
        /// The page ranges to export.
        ranges: Vec<Pages>,
    },
    /// Merge pages into a single page.
    Merge {
        /// The gap between pages (in pt).
        gap: Scalar,
    },
    /// Uses a pretty printer to format the output.
    Pretty {
        /// The pretty printer id provided by editor.
        /// If not provided, the default pretty printer will be used.
        /// Note: the builtin one may be only effective for json outputs.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        id: Option<String>,
    },
}

/// An export pdf task specifier.
#[toml_model]
pub struct ExportPdfTask {
    /// The shared export arguments
    #[serde(flatten)]
    pub export: ExportTask,
    /// The pdf standards.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub pdf_standards: Vec<PdfStandard>,
    /// The document's creation date formatted as a UNIX timestamp (in second
    /// unit).
    ///
    /// For more information, see <https://reproducible-builds.org/specs/source-date-epoch/>.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub creation_timestamp: Option<i64>,
}

/// An export png task specifier.
#[toml_model]
pub struct ExportPngTask {
    /// The shared export arguments
    #[serde(flatten)]
    pub export: ExportTask,
    /// The PPI (pixels per inch) to use for PNG export.
    pub ppi: Scalar,
    /// The background fill color (in typst script).
    /// e.g. `#ffffff`, `#000000`, `rgba(255, 255, 255, 0.5)`.
    ///
    /// If not provided, the default background color specified in the document
    /// will be used.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fill: Option<String>,
}

/// An export svg task specifier.
#[toml_model]
pub struct ExportSvgTask {
    /// The shared export arguments
    #[serde(flatten)]
    pub export: ExportTask,
}

/// An export html task specifier.
#[toml_model]
pub struct ExportHtmlTask {
    /// The shared export arguments
    #[serde(flatten)]
    pub export: ExportTask,
}

/// An export markdown task specifier.
#[toml_model]
pub struct ExportMarkdownTask {
    /// The shared export arguments
    #[serde(flatten)]
    pub export: ExportTask,
}

/// An export text task specifier.
#[toml_model]
pub struct ExportTextTask {
    /// The shared export arguments
    #[serde(flatten)]
    pub export: ExportTask,
}

/// An export query task specifier.
#[toml_model]
pub struct QueryTask {
    /// The shared export arguments
    #[serde(flatten)]
    pub export: ExportTask,
    /// The format to serialize in. Can be `json`, `yaml`, or `txt`,
    pub format: String,
    /// Specify a different output extension than the format.
    pub output_extension: String,
    /// Defines which elements to retrieve.
    pub selector: String,
    /// Extracts just one field from all retrieved elements.
    pub field: Option<String>,
    /// Expects and retrieves exactly one element.
    pub one: bool,
}
