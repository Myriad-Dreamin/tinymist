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
        doc: Option<Arc<TypstDocument>>,
        position_encoding: PositionEncoding,
    ) -> Option<CompletionResponse> {
        let doc = doc.as_deref();
        let source = get_suitable_source_in_workspace(world, &self.path).ok()?;
        let offset = lsp_to_typst::position(self.position, position_encoding, &source)?;
        // the typst's cursor is 1-based, so we need to add 1 to the offset
        let cursor = offset + 1;

        let (offset, completions) =
            typst_ide::autocomplete(world, doc, &source, cursor, self.explicit)?;

        let lsp_start_position =
            typst_to_lsp::offset_to_position(offset, position_encoding, &source);
        let replace_range = LspRange::new(lsp_start_position, self.position);
        Some(typst_to_lsp::completions(&completions, replace_range).into())
    }
}
