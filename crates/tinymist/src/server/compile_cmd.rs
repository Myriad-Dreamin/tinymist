use std::path::PathBuf;

use log::{error, info};
use lsp_types::ExecuteCommandParams;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use tinymist_query::{ExportKind, PageSelection};

use crate::{internal_error, invalid_params, method_not_found, run_query};

use super::compile::*;
use super::*;

#[derive(Debug, Clone, Default, Deserialize)]
struct ExportOpts {
    page: PageSelection,
}

impl CompileState {
    pub fn get_exec_commands() -> ExecuteCmdMap<Self> {
        type State = CompileState;

        ExecuteCmdMap::from_iter([
            exec_fn_!("tinymist.exportPdf", State::export_pdf),
            exec_fn_!("tinymist.exportSvg", State::export_svg),
            exec_fn_!("tinymist.exportPng", State::export_png),
            exec_fn!("tinymist.doClearCache", State::clear_cache),
            exec_fn!("tinymist.changeEntry", State::change_entry),
        ])
    }

    /// The entry point for the `workspace/executeCommand` request.
    pub fn execute_command(
        &mut self,
        req_id: RequestId,
        params: ExecuteCommandParams,
    ) -> ScheduledResult {
        let ExecuteCommandParams {
            command,
            arguments: args,
            work_done_progress_params: _,
        } = params;
        let Some(handler) = self.exec_cmds.get(command.as_str()) else {
            error!("asked to execute unknown command");
            return Err(method_not_found());
        };
        handler(self, req_id, args)
    }

    /// Export the current document as a PDF file.
    pub fn export_pdf(&mut self, req_id: RequestId, args: Vec<JsonValue>) -> ScheduledResult {
        self.export(req_id, ExportKind::Pdf, args)
    }

    /// Export the current document as a Svg file.
    pub fn export_svg(&mut self, req_id: RequestId, mut args: Vec<JsonValue>) -> ScheduledResult {
        let opts = get_arg_or_default!(args[1] as ExportOpts);
        self.export(req_id, ExportKind::Svg { page: opts.page }, args)
    }

    /// Export the current document as a Png file.
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
    ///
    /// # Errors
    /// Errors if the cache could not be cleared.
    pub fn clear_cache(&self, _arguments: Vec<JsonValue>) -> AnySchedulableResponse {
        comemo::evict(0);
        self.compiler().clear_cache();
        just_result!(JsonValue::Null)
    }

    /// Focus main file to some path.
    pub fn change_entry(&mut self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        let entry = get_arg!(args[0] as Option<PathBuf>).map(From::from);

        let update_result = self.do_change_entry(entry.clone());
        update_result.map_err(|err| internal_error(format!("could not focus file: {err}")))?;

        info!("entry changed: {entry:?}");
        just_result!(JsonValue::Null)
    }
}
