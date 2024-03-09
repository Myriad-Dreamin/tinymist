use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct HoverRequest {
    pub path: PathBuf,
    pub position: LspPosition,
}

impl HoverRequest {
    pub fn request(
        self,
        world: &TypstSystemWorld,
        doc: Option<Arc<TypstDocument>>,
        position_encoding: PositionEncoding,
    ) -> Option<Hover> {
        let source = get_suitable_source_in_workspace(world, &self.path).ok()?;
        let offset = lsp_to_typst::position(self.position, position_encoding, &source)?;
        // the typst's cursor is 1-based, so we need to add 1 to the offset
        let cursor = offset + 1;

        let typst_tooltip = typst_ide::tooltip(world, doc.as_deref(), &source, cursor)?;

        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        let range = typst_to_lsp::range(ast_node.range(), &source, position_encoding);

        Some(Hover {
            contents: typst_to_lsp::tooltip(&typst_tooltip),
            range: Some(range),
        })
    }
}
