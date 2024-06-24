use std::{collections::HashMap, path::PathBuf};

use log::{error, info};
use lsp_types::ExecuteCommandParams;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use tinymist_query::{ExportKind, PageSelection};

use crate::{internal_error, invalid_params, method_not_found, run_query, LspResult};

use super::compile::*;
use super::*;

macro_rules! exec_fn {
    ($ty: ty, Self::$method: ident, $($arg_key:ident),+ $(,)?) => {{
        const E: $ty = |this, $($arg_key),+| this.$method($($arg_key),+);
        E
    }};
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ExportOpts {
    page: PageSelection,
}

type ExecuteCmdMap = HashMap<&'static str, LspHandler<Vec<JsonValue>, JsonValue>>;

impl CompileState {
    pub fn get_exec_commands() -> ExecuteCmdMap {
        macro_rules! redirected_command {
            ($key: expr, Self::$method: ident) => {
                (
                    $key,
                    exec_fn!(LspHandler<Vec<JsonValue>, JsonValue>, Self::$method, inputs),
                )
            };
        }

        ExecuteCmdMap::from_iter([
            redirected_command!("tinymist.exportPdf", Self::export_pdf),
            redirected_command!("tinymist.exportSvg", Self::export_svg),
            redirected_command!("tinymist.exportPng", Self::export_png),
            redirected_command!("tinymist.doClearCache", Self::clear_cache),
            redirected_command!("tinymist.changeEntry", Self::change_entry),
        ])
    }

    /// The entry point for the `workspace/executeCommand` request.
    pub fn execute_command(&mut self, params: ExecuteCommandParams) -> LspResult<JsonValue> {
        let ExecuteCommandParams {
            command,
            arguments: args,
            work_done_progress_params: _,
        } = params;
        let Some(handler) = self.exec_cmds.get(command.as_str()) else {
            error!("asked to execute unknown command");
            return Err(method_not_found());
        };
        handler(self, args)
    }

    /// Export the current document as a PDF file.
    pub fn export_pdf(&self, args: Vec<JsonValue>) -> AnySchedulableResponse {
        self.export(ExportKind::Pdf, args)
    }

    /// Export the current document as a Svg file.
    pub fn export_svg(&self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        let opts = get_arg_or_default!(args[1] as ExportOpts);
        self.export(ExportKind::Svg { page: opts.page }, args)
    }

    /// Export the current document as a Png file.
    pub fn export_png(&self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        let opts = get_arg_or_default!(args[1] as ExportOpts);
        self.export(ExportKind::Png { page: opts.page }, args)
    }

    /// Export the current document as some format. The client is responsible
    /// for passing the correct absolute path of typst document.
    pub fn export(&self, kind: ExportKind, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        let path = get_arg!(args[0] as PathBuf);

        let res = run_query!(self.OnExport(path, kind))?;
        let res = serde_json::to_value(res).map_err(|_| internal_error("Cannot serialize path"))?;

        Ok(res)
    }

    /// Clear all cached resources.
    ///
    /// # Errors
    /// Errors if the cache could not be cleared.
    pub fn clear_cache(&self, _arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        comemo::evict(0);
        self.compiler().clear_cache();
        Ok(JsonValue::Null)
    }

    /// Focus main file to some path.
    pub fn change_entry(&mut self, mut args: Vec<JsonValue>) -> LspResult<JsonValue> {
        let entry = get_arg!(args[0] as Option<PathBuf>).map(From::from);

        let update_result = self.do_change_entry(entry.clone());
        update_result.map_err(|err| internal_error(format!("could not focus file: {err}")))?;

        info!("entry changed: {entry:?}");
        Ok(JsonValue::Null)
    }
}
