//! Tinymist LSP commands

use std::ops::Deref;
use std::path::PathBuf;

use lsp_server::RequestId;
use lsp_types::*;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use task::TraceParams;
use tinymist_assets::TYPST_PREVIEW_HTML;
use tinymist_query::{ExportKind, PageSelection};
use typst::diag::StrResult;
use typst::syntax::package::{PackageSpec, VersionlessPackageSpec};
use typst_ts_core::error::prelude::*;

use super::server::*;
use super::*;
use crate::tool::package::InitTask;

#[derive(Debug, Clone, Default, Deserialize)]
struct ExportOpts {
    page: PageSelection,
}

/// Here are implemented the handlers for each command.
impl LanguageState {
    /// Export the current document as PDF file(s).
    pub fn export_pdf(&mut self, req_id: RequestId, args: Vec<JsonValue>) -> ScheduledResult {
        self.export(req_id, ExportKind::Pdf, args)
    }

    /// Export the current document as Svg file(s).
    pub fn export_svg(&mut self, req_id: RequestId, mut args: Vec<JsonValue>) -> ScheduledResult {
        let opts = get_arg_or_default!(args[1] as ExportOpts);
        self.export(req_id, ExportKind::Svg { page: opts.page }, args)
    }

    /// Export the current document as Png file(s).
    pub fn export_png(&mut self, req_id: RequestId, mut args: Vec<JsonValue>) -> ScheduledResult {
        let opts = get_arg_or_default!(args[1] as ExportOpts);
        self.export(req_id, ExportKind::Png { page: opts.page }, args)
    }

    /// Export the current document as some format. The client is responsible
    /// for passing the correct absolute path of typst document.
    pub fn export(
        &mut self,
        req_id: RequestId,
        kind: ExportKind,
        mut args: Vec<JsonValue>,
    ) -> ScheduledResult {
        let path = get_arg!(args[0] as PathBuf);

        run_query!(req_id, self.OnExport(path, kind))
    }

    /// Clear all cached resources.
    pub fn clear_cache(&mut self, _arguments: Vec<JsonValue>) -> AnySchedulableResponse {
        comemo::evict(0);
        self.primary().clear_cache();
        just_ok(JsonValue::Null)
    }

    /// Pin main file to some path.
    pub fn pin_document(&mut self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        let entry = get_arg!(args[0] as Option<PathBuf>).map(From::from);

        let update_result = self.pin_entry(entry.clone());
        update_result.map_err(|err| internal_error(format!("could not pin file: {err}")))?;

        log::info!("file pinned: {entry:?}");
        just_ok(JsonValue::Null)
    }

    /// Focus main file to some path.
    pub fn focus_document(&mut self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        let entry = get_arg!(args[0] as Option<PathBuf>).map(From::from);

        if !self.ever_manual_focusing {
            self.ever_manual_focusing = true;
            log::info!("first manual focusing is coming");
        }

        let ok = self.focus_entry(entry.clone());
        let ok = ok.map_err(|err| internal_error(format!("could not focus file: {err}")))?;

        if ok {
            log::info!("file focused: {entry:?}");
        }
        just_ok(JsonValue::Null)
    }

    /// Start a preview instance.
    #[cfg(feature = "preview")]
    pub fn start_preview(
        &mut self,
        mut args: Vec<JsonValue>,
    ) -> SchedulableResponse<crate::tool::preview::StartPreviewResponse> {
        use std::path::Path;

        use crate::tool::preview::PreviewCliArgs;
        use clap::Parser;

        let cli_args = get_arg_or_default!(args[0] as Vec<String>);
        // clap parse
        let cli_args = ["preview"]
            .into_iter()
            .chain(cli_args.iter().map(|e| e.as_str()));
        let cli_args =
            PreviewCliArgs::try_parse_from(cli_args).map_err(|e| invalid_params(e.to_string()))?;

        // todo: preview specific arguments are not used
        let input = cli_args
            .compile
            .input
            .clone()
            .ok_or_else(|| internal_error("entry file must be provided"))?;
        let input = Path::new(&input);
        let entry = if input.is_absolute() {
            input.into()
        } else {
            // std::env::current_dir().unwrap().join(input)
            return Err(invalid_params("entry file must be absolute path"));
        };

        // todo: race condition
        let handle = self.primary().handle.clone();
        if handle.registered_preview() {
            return Err(internal_error("preview is already running"));
        }

        // todo: recover pin status reliably
        self.pin_entry(Some(entry))
            .map_err(|e| internal_error(format!("could not pin file: {e}")))?;

        self.preview.start(cli_args, handle)
    }

    /// Kill a preview instance.
    #[cfg(feature = "preview")]
    pub fn kill_preview(&mut self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        let task_id = get_arg!(args[0] as String);

        self.preview.kill(task_id)
    }

    /// Scroll preview instances.
    #[cfg(feature = "preview")]
    pub fn scroll_preview(&mut self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        use typst_preview::ControlPlaneMessage;

        let task_id = get_arg!(args[0] as String);
        let req = get_arg!(args[1] as ControlPlaneMessage);

        self.preview.scroll(task_id, req)
    }

    /// Initialize a new template.
    pub fn init_template(&mut self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        use crate::tool::package::{self, determine_latest_version, TemplateSource};

        #[derive(Debug, Serialize)]
        #[serde(rename_all = "camelCase")]
        struct InitResult {
            entry_path: PathBuf,
        }

        let from_source = get_arg!(args[0] as String);
        let to_path = get_arg!(args[1] as Option<PathBuf>).map(From::from);

        let snap = self.primary().snapshot().map_err(z_internal_error)?;

        just_future(async move {
            let snap = snap.snapshot().await.map_err(z_internal_error)?;

            // Parse the package specification. If the user didn't specify the version,
            // we try to figure it out automatically by downloading the package index
            // or searching the disk.
            let spec: PackageSpec = from_source
                .parse()
                .or_else(|err| {
                    // Try to parse without version, but prefer the error message of the
                    // normal package spec parsing if it fails.
                    let spec: VersionlessPackageSpec = from_source.parse().map_err(|_| err)?;
                    let version = determine_latest_version(&snap.world, &spec)?;
                    StrResult::Ok(spec.at(version))
                })
                .map_err(map_string_err("failed to parse package spec"))
                .map_err(z_internal_error)?;

            let from_source = TemplateSource::Package(spec);

            let entry_path = package::init(
                &snap.world,
                InitTask {
                    tmpl: from_source.clone(),
                    dir: to_path.clone(),
                },
            )
            .map_err(map_string_err("failed to initialize template"))
            .map_err(z_internal_error)?;

            log::info!("template initialized: {from_source:?} to {to_path:?}");

            serde_json::to_value(InitResult { entry_path })
                .map_err(|_| internal_error("Cannot serialize path"))
        })
    }

    /// Get the entry of a template.
    pub fn get_template_entry(&mut self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        use crate::tool::package::{self, determine_latest_version, TemplateSource};

        let from_source = get_arg!(args[0] as String);

        let snap = self.primary().snapshot().map_err(z_internal_error)?;

        just_future(async move {
            let snap = snap.snapshot().await.map_err(z_internal_error)?;

            // Parse the package specification. If the user didn't specify the version,
            // we try to figure it out automatically by downloading the package index
            // or searching the disk.
            let spec: PackageSpec = from_source
                .parse()
                .or_else(|err| {
                    // Try to parse without version, but prefer the error message of the
                    // normal package spec parsing if it fails.
                    let spec: VersionlessPackageSpec = from_source.parse().map_err(|_| err)?;
                    let version = determine_latest_version(&snap.world, &spec)?;
                    StrResult::Ok(spec.at(version))
                })
                .map_err(map_string_err("failed to parse package spec"))
                .map_err(z_internal_error)?;

            let from_source = TemplateSource::Package(spec);

            let entry = package::get_entry(&snap.world, from_source)
                .map_err(map_string_err("failed to get template entry"))
                .map_err(z_internal_error)?;

            let entry = String::from_utf8(entry.to_vec())
                .map_err(|_| invalid_params("template entry is not a valid UTF-8 string"))?;

            Ok(JsonValue::String(entry))
        })
    }

    /// Interact with the code context at the source file.
    pub fn interact_code_context(
        &mut self,
        req_id: RequestId,
        _arguments: Vec<JsonValue>,
    ) -> ScheduledResult {
        let queries = _arguments.into_iter().next().ok_or_else(|| {
            invalid_params("The first parameter is not a valid code context query array")
        })?;

        #[derive(Debug, Clone, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct InteractCodeContextParams {
            pub text_document: TextDocumentIdentifier,
            pub query: Vec<tinymist_query::InteractCodeContextQuery>,
        }

        let params: InteractCodeContextParams = serde_json::from_value(queries)
            .map_err(|e| invalid_params(format!("Cannot parse code context queries: {e}")))?;
        let path = as_path(params.text_document);
        let query = params.query;

        run_query!(req_id, self.InteractCodeContext(path, query))
    }

    /// Get the trace data of the document.
    pub fn get_document_trace(&mut self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        let path = get_arg!(args[0] as PathBuf).into();

        // get path to self program
        let self_path = std::env::current_exe()
            .map_err(|e| internal_error(format!("Cannot get typst compiler {e}")))?;

        let entry = self.config.compile.determine_entry(Some(path));

        let snap = self.primary().snapshot().map_err(z_internal_error)?;
        let user_action = self.user_action;

        just_future(async move {
            let snap = snap.snapshot().await.map_err(z_internal_error)?;
            let display_entry = || format!("{entry:?}");

            // todo: rootless file
            // todo: memory dirty file
            let root = entry
                .root()
                .ok_or_else(|| error_once!("root must be determined for trace, got", entry: display_entry()))
                .map_err(z_internal_error)?;
            let main = entry
                .main()
                .and_then(|e| e.vpath().resolve(&root))
                .ok_or_else(
                    || error_once!("main file must be resolved, got", entry: display_entry()),
                )
                .map_err(z_internal_error)?;

            let task = user_action.trace(TraceParams {
                compiler_program: self_path,
                root: root.as_ref().to_owned(),
                main,
                inputs: snap.world.inputs().as_ref().deref().clone(),
                font_paths: snap.world.font_resolver.font_paths().to_owned(),
            })?;

            tokio::pin!(task);
            task.as_mut().await;
            let resp = task.take_output().unwrap()?;

            serde_json::to_value(resp).map_err(|e| internal_error(e.to_string()))
        })
    }

    /// Get the metrics of the document.
    pub fn get_document_metrics(
        &mut self,
        req_id: RequestId,
        mut args: Vec<JsonValue>,
    ) -> ScheduledResult {
        let path = get_arg!(args[0] as PathBuf);
        run_query!(req_id, self.DocumentMetrics(path))
    }

    /// Get the server info.
    pub fn get_server_info(
        &mut self,
        req_id: RequestId,
        _arguments: Vec<JsonValue>,
    ) -> ScheduledResult {
        run_query!(req_id, self.ServerInfo())
    }
}

impl LanguageState {
    /// Get the all valid symbols
    pub fn resource_symbols(&mut self, _arguments: Vec<JsonValue>) -> AnySchedulableResponse {
        let snapshot = self.primary().snapshot().map_err(z_internal_error)?;
        just_future(Self::get_symbol_resources(snapshot))
    }

    /// Get resource preview html
    pub fn resource_preview_html(&mut self, _arguments: Vec<JsonValue>) -> AnySchedulableResponse {
        let resp = serde_json::to_value(TYPST_PREVIEW_HTML);
        just_result(resp.map_err(|e| internal_error(e.to_string())))
    }

    /// Get tutorial web page
    pub fn resource_tutoral(&mut self, _arguments: Vec<JsonValue>) -> AnySchedulableResponse {
        Err(method_not_found())
    }
}
