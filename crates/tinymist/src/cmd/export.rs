//! Tinymist LSP commands for export

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value as JsonValue;
use tinymist_project::{
    ExportHtmlTask, ExportPdfTask, ExportPngTask, ExportSvgTask, ExportTeXTask, ExportTextTask,
    Pages, ProjectTask, QueryTask,
};
use tinymist_std::error::prelude::*;
use tinymist_task::{ExportMarkdownTask, PageMerge};

use super::*;
use crate::lsp::query::run_query;

/// Basic export options with no additional fields.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
#[serde(rename_all = "camelCase")]
struct ExportOpts {}

/// See [`ProjectTask`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ExportPdfOpts {
    /// Which pages to export. When unspecified, all pages are exported.
    pages: Option<Vec<Pages>>,
    /// The creation timestamp for various outputs (in seconds).
    creation_timestamp: Option<String>,
    /// A PDF standard that Typst can enforce conformance with.
    pdf_standard: Option<Vec<PdfStandard>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ExportSvgOpts {
    /// Which pages to export. When unspecified, all pages are exported.
    pages: Option<Vec<Pages>>,
    page_number_template: Option<String>,
    merge: Option<PageMerge>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ExportPngOpts {
    /// Which pages to export. When unspecified, all pages are exported.
    pages: Option<Vec<Pages>>,
    page_number_template: Option<String>,
    merge: Option<PageMerge>,
    fill: Option<String>,
    ppi: Option<f32>,
}

/// See [`ProjectTask`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ExportTypliteOpts {
    /// The processor to use for the typlite export.
    processor: Option<String>,
    /// The path of external assets directory.
    assets_path: Option<PathBuf>,
}

/// See [`ProjectTask`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ExportQueryOpts {
    format: String,
    output_extension: Option<String>,
    strict: Option<bool>,
    pretty: Option<bool>,
    selector: String,
    field: Option<String>,
    one: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ExportActionOpts {
    /// Whether to write to file.
    write: Option<bool>,
    /// Whether to open the exported file(s) after the export is done.
    open: bool,
}

/// Here are implemented the handlers for each command.
impl ServerState {
    /// Export the current document as PDF file(s).
    pub fn export_pdf(&mut self, mut args: Vec<JsonValue>) -> ScheduleResult {
        let opts = get_arg_or_default!(args[1] as ExportPdfOpts);

        let creation_timestamp = if let Some(value) = opts.creation_timestamp {
            Some(
                parse_source_date_epoch(&value)
                    .map_err(|e| invalid_params(format!("Cannot parse creation timestamp: {e}")))?,
            )
        } else {
            self.config.creation_timestamp()
        };
        let pdf_standards = opts.pdf_standard.or_else(|| self.config.pdf_standards());

        let export = self.config.export_task();
        self.export(
            ProjectTask::ExportPdf(ExportPdfTask {
                export,
                pages: opts.pages,
                pdf_standards: pdf_standards.unwrap_or_default(),
                creation_timestamp,
            }),
            args,
        )
    }

    /// Export the current document as HTML file(s).
    pub fn export_html(&mut self, mut args: Vec<JsonValue>) -> ScheduleResult {
        let _opts = get_arg_or_default!(args[1] as ExportOpts);
        let export = self.config.export_task();
        self.export(ProjectTask::ExportHtml(ExportHtmlTask { export }), args)
    }

    /// Export the current document as Markdown file(s).
    pub fn export_markdown(&mut self, mut args: Vec<JsonValue>) -> ScheduleResult {
        let opts = get_arg_or_default!(args[1] as ExportTypliteOpts);
        let export = self.config.export_task();
        self.export(
            ProjectTask::ExportMd(ExportMarkdownTask {
                processor: opts.processor,
                assets_path: opts.assets_path,
                export,
            }),
            args,
        )
    }

    /// Export the current document as Tex file(s).
    pub fn export_tex(&mut self, mut args: Vec<JsonValue>) -> ScheduleResult {
        let opts = get_arg_or_default!(args[1] as ExportTypliteOpts);
        let export = self.config.export_task();
        self.export(
            ProjectTask::ExportTeX(ExportTeXTask {
                processor: opts.processor,
                assets_path: opts.assets_path,
                export,
            }),
            args,
        )
    }

    /// Export the current document as Text file(s).
    pub fn export_text(&mut self, mut args: Vec<JsonValue>) -> ScheduleResult {
        let _opts = get_arg_or_default!(args[1] as ExportOpts);
        let export = self.config.export_task();
        self.export(ProjectTask::ExportText(ExportTextTask { export }), args)
    }

    /// Query the current document and export the result as JSON file(s).
    pub fn export_query(&mut self, mut args: Vec<JsonValue>) -> ScheduleResult {
        let opts = get_arg_or_default!(args[1] as ExportQueryOpts);
        // todo: deprecate it
        let _ = opts.strict;

        let mut export = self.config.export_task();
        if opts.pretty.unwrap_or(true) {
            export.apply_pretty();
        }

        self.export(
            ProjectTask::Query(QueryTask {
                format: opts.format,
                output_extension: opts.output_extension,
                selector: opts.selector,
                field: opts.field,
                one: opts.one.unwrap_or(false),
                export,
            }),
            args,
        )
    }

    /// Export the current document as Svg file(s).
    pub fn export_svg(&mut self, mut args: Vec<JsonValue>) -> ScheduleResult {
        let opts = get_arg_or_default!(args[1] as ExportSvgOpts);

        let export = self.config.export_task();
        self.export(
            ProjectTask::ExportSvg(ExportSvgTask {
                export,
                pages: opts.pages,
                page_number_template: opts.page_number_template,
                merge: opts.merge,
            }),
            args,
        )
    }

    /// Export the current document as Png file(s).
    pub fn export_png(&mut self, mut args: Vec<JsonValue>) -> ScheduleResult {
        let opts = get_arg_or_default!(args[1] as ExportPngOpts);

        let ppi = opts.ppi.unwrap_or(144.);
        let ppi = ppi
            .try_into()
            .context("cannot convert ppi")
            .map_err(invalid_params)?;

        let export = self.config.export_task();
        self.export(
            ProjectTask::ExportPng(ExportPngTask {
                export,
                pages: opts.pages,
                page_number_template: opts.page_number_template,
                merge: opts.merge,
                fill: opts.fill,
                ppi,
            }),
            args,
        )
    }

    /// Export the current document as some format. The client is responsible
    /// for passing the correct absolute path of typst document.
    pub fn export(&mut self, task: ProjectTask, mut args: Vec<JsonValue>) -> ScheduleResult {
        let path = get_arg!(args[0] as PathBuf);
        let action_opts = get_arg_or_default!(args[2] as ExportActionOpts);
        let write = action_opts.write.unwrap_or(true);
        let open = action_opts.open;

        run_query!(self.OnExport(path, task, write, open))
    }
}
