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
        let source = get_suitable_source_in_workspace(world, &self.path).ok()?;
        let typst_offset = lsp_to_typst::position(self.position, position_encoding, &source)?;

        let (typst_start_offset, completions) =
            typst_ide::autocomplete(world, doc.as_deref(), &source, typst_offset, self.explicit)?;

        let lsp_start_position =
            typst_to_lsp::offset_to_position(typst_start_offset, position_encoding, &source);
        let replace_range = LspRange::new(lsp_start_position, self.position);
        Some(typst_to_lsp::completions(&completions, replace_range).into())
    }
}
