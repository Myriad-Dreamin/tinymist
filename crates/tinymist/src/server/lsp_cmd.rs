//! tinymist LSP mode

use std::ops::Deref;
use std::path::Path;
use std::{collections::HashMap, path::PathBuf};

use anyhow::{bail, Context};
use log::{error, info};
use lsp_server::RequestId;
use lsp_types::*;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tinymist_query::ExportKind;
use typst::diag::StrResult;
use typst::syntax::package::{PackageSpec, VersionlessPackageSpec};
use typst_ts_core::path::PathClean;
use typst_ts_core::{error::prelude::*, ImmutPath};

use crate::actor::user_action::{TraceParams, UserActionRequest};
use crate::tools::package::InitTask;
use crate::{run_query, LspResult};

use super::lsp::*;
use super::*;

macro_rules! exec_fn_ {
    ($key: expr, Self::$method: ident) => {
        ($key, {
            const E: LspRawHandler<Vec<JsonValue>> = |this, req_id, req| this.$method(req_id, req);
            E
        })
    };
}

macro_rules! exec_fn {
    ($key: expr, Self::$method: ident) => {
        ($key, {
            const E: LspRawHandler<Vec<JsonValue>> = |this, req_id, args| {
                let res = this.$method(args);
                this.client.respond(result_to_response(req_id, res));
                Ok(Some(()))
            };
            E
        })
    };
}

macro_rules! resource_fn {
    ($ty: ty, Self::$method: ident, $($arg_key:ident),+ $(,)?) => {{
        const E: $ty = |this, $($arg_key),+| this.$method($($arg_key),+);
        E
    }};
}

type LspHandler<Req, Res> = fn(srv: &mut LanguageState, args: Req) -> LspResult<Res>;

/// Returns Ok(Some()) -> Already responded
/// Returns Ok(None) -> Need to respond none
/// Returns Err(..) -> Need to respond error
type LspRawHandler<T> =
    fn(srv: &mut LanguageState, req_id: RequestId, args: T) -> LspResult<Option<()>>;

type ExecuteCmdMap = HashMap<&'static str, LspRawHandler<Vec<JsonValue>>>;
type ResourceMap = HashMap<ImmutPath, LspHandler<Vec<JsonValue>, JsonValue>>;

/// Here are implemented the handlers for each command.
impl LanguageState {
    pub fn get_exec_commands() -> ExecuteCmdMap {
        ExecuteCmdMap::from_iter([
            exec_fn!("tinymist.exportPdf", Self::export_pdf),
            exec_fn!("tinymist.exportSvg", Self::export_svg),
            exec_fn!("tinymist.exportPng", Self::export_png),
            exec_fn!("tinymist.doClearCache", Self::clear_cache),
            exec_fn!("tinymist.pinMain", Self::pin_document),
            exec_fn!("tinymist.focusMain", Self::focus_document),
            exec_fn!("tinymist.doInitTemplate", Self::init_template),
            exec_fn!("tinymist.doGetTemplateEntry", Self::do_get_template_entry),
            exec_fn!("tinymist.interactCodeContext", Self::interact_code_context),
            exec_fn_!("tinymist.getDocumentTrace", Self::get_document_trace),
            exec_fn!("tinymist.getDocumentMetrics", Self::get_document_metrics),
            exec_fn!("tinymist.getServerInfo", Self::get_server_info),
            // For Documentations
            exec_fn!("tinymist.getResources", Self::get_resources),
        ])
    }

    /// Export the current document as a PDF file.
    pub fn export_pdf(&mut self, args: Vec<JsonValue>) -> LspResult<JsonValue> {
        self.primary.export_pdf(args)
    }

    /// Export the current document as a Svg file.
    pub fn export_svg(&mut self, args: Vec<JsonValue>) -> LspResult<JsonValue> {
        self.primary.export_svg(args)
    }

    /// Export the current document as a Png file.
    pub fn export_png(&mut self, args: Vec<JsonValue>) -> LspResult<JsonValue> {
        self.primary.export_png(args)
    }

    /// Export the current document as some format. The client is responsible
    /// for passing the correct absolute path of typst document.
    pub fn export(&mut self, kind: ExportKind, args: Vec<JsonValue>) -> LspResult<JsonValue> {
        self.primary.export(kind, args)
    }

    /// Clear all cached resources.
    ///
    /// # Errors
    /// Errors if the cache could not be cleared.
    pub fn clear_cache(&self, _arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        comemo::evict(0);
        for v in Some(self.primary())
            .into_iter()
            .chain(self.dedicates.iter().map(|v| v.compiler()))
        {
            v.clear_cache();
        }
        Ok(JsonValue::Null)
    }

    /// Pin main file to some path.
    pub fn pin_document(&mut self, mut args: Vec<JsonValue>) -> LspResult<JsonValue> {
        let entry = get_arg!(args[0] as Option<PathBuf>).map(From::from);

        let update_result = self.pin_entry(entry.clone());
        update_result.map_err(|err| internal_error(format!("could not pin file: {err}")))?;

        info!("file pinned: {entry:?}");
        Ok(JsonValue::Null)
    }

    /// Focus main file to some path.
    pub fn focus_document(&mut self, mut args: Vec<JsonValue>) -> LspResult<JsonValue> {
        let entry = get_arg!(args[0] as Option<PathBuf>).map(From::from);

        if !self.ever_manual_focusing {
            self.ever_manual_focusing = true;
            log::info!("first manual focusing is coming");
        }

        let ok = self.focus_entry(entry.clone());
        let ok = ok.map_err(|err| internal_error(format!("could not focus file: {err}")))?;

        if ok {
            info!("file focused: {entry:?}");
        }
        Ok(JsonValue::Null)
    }

    /// Initialize a new template.
    pub fn init_template(&self, mut args: Vec<JsonValue>) -> LspResult<JsonValue> {
        use crate::tools::package::{self, determine_latest_version, TemplateSource};

        #[derive(Debug, Serialize)]
        #[serde(rename_all = "camelCase")]
        struct InitResult {
            entry_path: PathBuf,
        }

        let from_source = get_arg!(args[0] as String);
        let to_path = get_arg!(args[1] as Option<PathBuf>).map(From::from);
        let res = self
            .primary()
            .steal(move |c| {
                let world = c.verse.spawn();
                // Parse the package specification. If the user didn't specify the version,
                // we try to figure it out automatically by downloading the package index
                // or searching the disk.
                let spec: PackageSpec = from_source
                    .parse()
                    .or_else(|err| {
                        // Try to parse without version, but prefer the error message of the
                        // normal package spec parsing if it fails.
                        let spec: VersionlessPackageSpec = from_source.parse().map_err(|_| err)?;
                        let version = determine_latest_version(&c.verse, &spec)?;
                        StrResult::Ok(spec.at(version))
                    })
                    .map_err(map_string_err("failed to parse package spec"))?;

                let from_source = TemplateSource::Package(spec);

                let entry_path = package::init(
                    &world,
                    InitTask {
                        tmpl: from_source.clone(),
                        dir: to_path.clone(),
                    },
                )
                .map_err(map_string_err("failed to initialize template"))?;

                info!("template initialized: {from_source:?} to {to_path:?}");

                ZResult::Ok(InitResult { entry_path })
            })
            .and_then(|e| e)
            .map_err(|e| invalid_params(format!("failed to determine template source: {e}")))?;

        serde_json::to_value(res).map_err(|_| internal_error("Cannot serialize path"))
    }

    /// Get the entry of a template.
    pub fn do_get_template_entry(&self, mut args: Vec<JsonValue>) -> LspResult<JsonValue> {
        use crate::tools::package::{self, determine_latest_version, TemplateSource};

        let from_source = get_arg!(args[0] as String);
        let entry = self
            .primary()
            .steal(move |c| {
                // Parse the package specification. If the user didn't specify the version,
                // we try to figure it out automatically by downloading the package index
                // or searching the disk.
                let spec: PackageSpec = from_source
                    .parse()
                    .or_else(|err| {
                        // Try to parse without version, but prefer the error message of the
                        // normal package spec parsing if it fails.
                        let spec: VersionlessPackageSpec = from_source.parse().map_err(|_| err)?;
                        let version = determine_latest_version(&c.verse, &spec)?;
                        StrResult::Ok(spec.at(version))
                    })
                    .map_err(map_string_err("failed to parse package spec"))?;

                let from_source = TemplateSource::Package(spec);

                let entry = package::get_entry(&c.verse, from_source)
                    .map_err(map_string_err("failed to get template entry"))?;

                ZResult::Ok(entry)
            })
            .and_then(|e| e)
            .map_err(|e| invalid_params(format!("failed to determine template entry: {e}")))?;

        let entry = String::from_utf8(entry.to_vec())
            .map_err(|_| invalid_params("template entry is not a valid UTF-8 string"))?;

        Ok(JsonValue::String(entry))
    }

    /// Interact with the code context at the source file.
    pub fn interact_code_context(&mut self, _arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
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

        let res = run_query!(self.InteractCodeContext(path, query))?;
        let res =
            serde_json::to_value(res).map_err(|_| internal_error("Cannot serialize responses"))?;

        Ok(res)
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

        let res = self
            .primary()
            .steal(move |c| {
                let verse = &c.verse;

                // todo: rootless file
                // todo: memory dirty file
                let root = entry.root().ok_or_else(|| {
                    anyhow::anyhow!("root must be determined for trace, got {entry:?}")
                })?;
                let main = entry
                    .main()
                    .and_then(|e| e.vpath().resolve(&root))
                    .ok_or_else(|| anyhow::anyhow!("main file must be resolved, got {entry:?}"))?;

                if let Some(f) = thread {
                    f.send(UserActionRequest::Trace(
                        req_id,
                        TraceParams {
                            compiler_program: self_path,
                            root: root.as_ref().to_owned(),
                            main,
                            inputs: verse.inputs().as_ref().deref().clone(),
                            font_paths: verse.font_resolver.font_paths().to_owned(),
                        },
                    ))
                    .context("cannot send trace request")?;
                } else {
                    bail!("user action thread is not available");
                }

                Ok(Some(()))
            })
            .context("cannot steal primary compiler");

        let res = match res {
            Ok(res) => res,
            Err(res) => Err(res),
        };

        res.map_err(|e| internal_error(format!("could not get document trace: {e}")))
    }

    /// Get the metrics of the document.
    pub fn get_document_metrics(&mut self, mut args: Vec<JsonValue>) -> LspResult<JsonValue> {
        let path = get_arg!(args[0] as PathBuf);

        let res = run_query!(self.DocumentMetrics(path))?;
        let res = serde_json::to_value(res)
            .map_err(|e| internal_error(format!("Cannot serialize response {e}")))?;

        Ok(res)
    }

    /// Get the server info.
    pub fn get_server_info(&mut self, _arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        let res = run_query!(self.ServerInfo())?;

        let res = serde_json::to_value(res)
            .map_err(|e| internal_error(format!("Cannot serialize response {e}")))?;

        Ok(res)
    }

    /// Get static resources with help of tinymist service, for example, a
    /// static help pages for some typst function.
    pub fn get_resources(&mut self, mut args: Vec<JsonValue>) -> LspResult<JsonValue> {
        let path = get_arg!(args[0] as PathBuf);

        let Some(handler) = self.resource_routes.get(path.as_path()) else {
            error!("asked for unknown resource: {path:?}");
            return Err(method_not_found());
        };

        // Note our redirection will keep the first path argument in the args vec.
        handler(self, args)
    }
}

impl LanguageState {
    pub fn get_resource_routes() -> ResourceMap {
        macro_rules! resources_at {
            ($key: expr, Self::$method: ident) => {
                (
                    Path::new($key).clean().as_path().into(),
                    resource_fn!(LspHandler<Vec<JsonValue>, JsonValue>, Self::$method, inputs),
                )
            };
        }

        ResourceMap::from_iter([
            resources_at!("/symbols", Self::resource_symbols),
            resources_at!("/tutorial", Self::resource_tutoral),
        ])
    }

    /// Get the all valid symbols
    pub fn resource_symbols(&self, _arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        let resp = self.get_symbol_resources();
        resp.map_err(|e| internal_error(e.to_string()))
    }

    /// Get tutorial web page
    pub fn resource_tutoral(&self, _arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        Err(method_not_found())
    }
}
