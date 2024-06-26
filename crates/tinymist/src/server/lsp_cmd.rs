//! tinymist LSP mode

use std::ops::Deref;
use std::path::PathBuf;

use lsp_server::RequestId;
use lsp_types::*;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tinymist_query::ExportKind;
use typst::diag::StrResult;
use typst::syntax::package::{PackageSpec, VersionlessPackageSpec};
use typst_ts_core::error::prelude::*;

use super::lsp::*;
use super::*;
use crate::actor::user_action::{TraceParams, UserActionRequest};
use crate::tools::package::InitTask;

/// Here are implemented the handlers for each command.
impl LanguageState {
    /// Export the current document as a PDF file.
    pub fn export_pdf(&mut self, req_id: RequestId, args: Vec<JsonValue>) -> ScheduledResult {
        self.primary.export_pdf(req_id, args)
    }

    /// Export the current document as a Svg file.
    pub fn export_svg(&mut self, req_id: RequestId, args: Vec<JsonValue>) -> ScheduledResult {
        self.primary.export_svg(req_id, args)
    }

    /// Export the current document as a Png file.
    pub fn export_png(&mut self, req_id: RequestId, args: Vec<JsonValue>) -> ScheduledResult {
        self.primary.export_png(req_id, args)
    }

    /// Export the current document as some format. The client is responsible
    /// for passing the correct absolute path of typst document.
    pub fn export(
        &mut self,
        req_id: RequestId,
        kind: ExportKind,
        args: Vec<JsonValue>,
    ) -> ScheduledResult {
        self.primary.export(req_id, kind, args)
    }

    /// Clear all cached resources.
    ///
    /// # Errors
    /// Errors if the cache could not be cleared.
    pub fn clear_cache(&mut self, _arguments: Vec<JsonValue>) -> AnySchedulableResponse {
        comemo::evict(0);
        for v in Some(self.primary())
            .into_iter()
            .chain(self.dedicates.iter().map(|v| v.compiler()))
        {
            v.clear_cache();
        }
        just_ok!(JsonValue::Null)
    }

    /// Pin main file to some path.
    pub fn pin_document(&mut self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        let entry = get_arg!(args[0] as Option<PathBuf>).map(From::from);

        let update_result = self.pin_entry(entry.clone());
        update_result.map_err(|err| internal_error(format!("could not pin file: {err}")))?;

        log::info!("file pinned: {entry:?}");
        just_ok!(JsonValue::Null)
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
        just_ok!(JsonValue::Null)
    }

    /// Initialize a new template.
    pub fn init_template(&mut self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        use crate::tools::package::{self, determine_latest_version, TemplateSource};

        #[derive(Debug, Serialize)]
        #[serde(rename_all = "camelCase")]
        struct InitResult {
            entry_path: PathBuf,
        }

        let from_source = get_arg!(args[0] as String);
        let to_path = get_arg!(args[1] as Option<PathBuf>).map(From::from);

        let snap = self.primary().sync_snapshot().map_err(z_internal_error)?;

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

        let res = serde_json::to_value(InitResult { entry_path })
            .map_err(|_| internal_error("Cannot serialize path"));
        just_result!(res)
    }

    /// Get the entry of a template.
    pub fn do_get_template_entry(&mut self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        use crate::tools::package::{self, determine_latest_version, TemplateSource};

        let from_source = get_arg!(args[0] as String);

        let snap = self.primary().sync_snapshot().map_err(z_internal_error)?;

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

        just_ok!(JsonValue::String(entry))
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
    pub fn get_document_trace(
        &mut self,
        req_id: RequestId,
        mut args: Vec<JsonValue>,
    ) -> LspResult<Option<()>> {
        let path = get_arg!(args[0] as PathBuf).into();

        // get path to self program
        let self_path = std::env::current_exe()
            .map_err(|e| internal_error(format!("Cannot get typst compiler {e}")))?;

        let thread = self.user_action_thread.clone();
        let entry = self.config.compile.determine_entry(Some(path));

        let snap = self.primary().sync_snapshot().map_err(z_internal_error)?;

        // todo: rootless file
        // todo: memory dirty file
        let root = entry.root().ok_or_else(
            || error_once!("root must be determined for trace, got", entry: format!("{entry:?}")),
        ).map_err(z_internal_error)?;
        let main = entry
            .main()
            .and_then(|e| e.vpath().resolve(&root))
            .ok_or_else(
                || error_once!("main file must be resolved, got", entry: format!("{entry:?}")),
            )
            .map_err(z_internal_error)?;

        let Some(f) = thread else {
            return Err(internal_error("user action thread is not available"))?;
        };

        f.send(UserActionRequest::Trace(
            req_id,
            TraceParams {
                compiler_program: self_path,
                root: root.as_ref().to_owned(),
                main,
                inputs: snap.world.inputs().as_ref().deref().clone(),
                font_paths: snap.world.font_resolver.font_paths().to_owned(),
            },
        ))
        .map_err(|_| internal_error("cannot send trace request"))
        .map(Some)
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
        let resp = self.get_symbol_resources();
        just_ok!(resp.map_err(|e| internal_error(e.to_string()))?)
    }

    /// Get tutorial web page
    pub fn resource_tutoral(&mut self, _arguments: Vec<JsonValue>) -> AnySchedulableResponse {
        Err(method_not_found())
    }
}
