//! Tinymist LSP commands: pinning and focusing documents.

use super::*;

type Selections = Vec<LspRange>;

/// Extra options for the focus command.
#[derive(Debug, Clone, Default, Deserialize)]
struct FocusDocOpts {
    /// An optional list of selections to be set after focusing. The first
    /// selection is the primary one.
    #[serde(default)]
    selections: Option<Selections>,
}

impl ServerState {
    /// Pins main file to some path.
    pub fn pin_document(&mut self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        let entry = get_arg!(args[0] as Option<PathBuf>).map(From::from);
        let opts = get_arg_or_default!(args[1] as FocusDocOpts);

        let update_result = self.pin_main_file(entry.clone());
        update_result.map_err(|err| internal_error(format!("could not pin file: {err}")))?;

        self.do_change_selections(opts.selections);

        log::info!("file pinned: {entry:?}");
        just_ok(JsonValue::Null)
    }

    /// Focuses main file to some path.
    pub fn focus_document(&mut self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        let entry = get_arg!(args[0] as Option<PathBuf>).map(From::from);

        if !self.ever_manual_focusing {
            self.ever_manual_focusing = true;
            log::info!("first manual focusing is coming");
        }

        let ok = self.focus_main_file(entry.clone());
        let ok = ok.map_err(|err| internal_error(format!("could not focus file: {err}")))?;

        if ok {
            log::info!("file focused: {entry:?}");
        }
        just_ok(JsonValue::Null)
    }

    /// Changes the selections.
    pub fn change_selections(&mut self, mut args: Vec<JsonValue>) -> AnySchedulableResponse {
        let selections = get_arg!(args[0] as Selections);
        self.do_change_selections(Some(selections));
        just_ok(JsonValue::Null)
    }
}
