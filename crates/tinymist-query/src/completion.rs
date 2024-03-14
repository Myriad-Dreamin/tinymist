use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub path: PathBuf,
    pub position: LspPosition,
    pub explicit: bool,
}

impl CompletionRequest {
    pub fn request(
        self,
        world: &TypstSystemWorld,
        doc: Option<VersionedDocument>,
        position_encoding: PositionEncoding,
    ) -> Option<CompletionResponse> {
        let doc = doc.as_ref().map(|doc| doc.document.as_ref());
        let source = get_suitable_source_in_workspace(world, &self.path).ok()?;
        let cursor = lsp_to_typst::position(self.position, position_encoding, &source)?;

        // Please see <https://github.com/nvarner/typst-lsp/commit/2d66f26fb96ceb8e485f492e5b81e9db25c3e8ec>
        //
        // FIXME: correctly identify a completion which is triggered
        // by explicit action, such as by pressing control and space
        // or something similar.
        //
        // See <https://github.com/microsoft/language-server-protocol/issues/1101>
        // > As of LSP 3.16, CompletionTriggerKind takes the value Invoked for
        // > both manually invoked (for ex: ctrl + space in VSCode) completions
        // > and always on (what the spec refers to as 24/7 completions).
        //
        // Hence, we cannot distinguish between the two cases. Conservatively, we
        // assume that the completion is not explicit.
        let explicit = false;

        let (offset, completions) = typst_ide::autocomplete(world, doc, &source, cursor, explicit)?;

        let lsp_start_position =
            typst_to_lsp::offset_to_position(offset, position_encoding, &source);
        let replace_range = LspRange::new(lsp_start_position, self.position);
        Some(typst_to_lsp::completions(&completions, replace_range).into())
    }
}
