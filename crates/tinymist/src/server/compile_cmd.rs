use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value as JsonValue;
use tinymist_query::{ExportKind, PageSelection};

use super::compile::*;
use super::*;

#[derive(Debug, Clone, Default, Deserialize)]
struct ExportOpts {
    page: PageSelection,
}

impl CompileState {
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
    pub fn clear_cache(&mut self, _arguments: Vec<JsonValue>) -> AnySchedulableResponse {
        comemo::evict(0);
        self.compiler().clear_cache();
        just_ok!(JsonValue::Null)
    }

    /// Focus main file to some path.
    pub fn change_entry(&mut self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        let entry = get_arg!(args[0] as Option<PathBuf>).map(From::from);

        let update_result = self.do_change_entry(entry.clone());
        update_result.map_err(|err| internal_error(format!("could not focus file: {err}")))?;

        log::info!("entry changed: {entry:?}");
        just_ok!(JsonValue::Null)
    }
}
