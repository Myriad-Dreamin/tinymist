use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub path: PathBuf,
    pub position: LspPosition,
    pub position_encoding: PositionEncoding,
    pub explicit: bool,
}

pub fn completion(
    world: &TypstSystemWorld,
    doc: Option<Arc<TypstDocument>>,
    req: CompletionRequest,
) -> Option<CompletionResponse> {
    let source = get_suitable_source_in_workspace(world, &req.path).ok()?;
    let typst_offset =
        lsp_to_typst::position_to_offset(req.position, req.position_encoding, &source);

    let (typst_start_offset, completions) =
        typst_ide::autocomplete(world, doc.as_deref(), &source, typst_offset, req.explicit)?;

    let lsp_start_position =
        typst_to_lsp::offset_to_position(typst_start_offset, req.position_encoding, &source);
    let replace_range = LspRawRange::new(lsp_start_position, req.position);
    Some(typst_to_lsp::completions(&completions, replace_range).into())
}
