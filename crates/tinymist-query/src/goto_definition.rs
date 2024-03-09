use comemo::Track;
use log::debug;
use tower_lsp::lsp_types::LocationLink;

use crate::{analysis::find_definition, prelude::*};

#[derive(Debug, Clone)]
pub struct GotoDefinitionRequest {
    pub path: PathBuf,
    pub position: LspPosition,
}

impl GotoDefinitionRequest {
    pub fn request(
        self,
        world: &TypstSystemWorld,
        position_encoding: PositionEncoding,
    ) -> Option<GotoDefinitionResponse> {
        let source = get_suitable_source_in_workspace(world, &self.path).ok()?;
        let typst_offset = lsp_to_typst::position(self.position, position_encoding, &source)?;

        let ast_node = LinkedNode::new(source.root()).leaf_at(typst_offset + 1)?;

        let t: &dyn World = world;

        let def = find_definition(t.track(), source.id(), ast_node)?;
        // todo: handle other definitions
        let span = def.span();
        let use_site = def.use_site();

        if span.is_detached() {
            return None;
        }

        let Some(id) = span.id() else {
            return None;
        };

        let origin_selection_range =
            typst_to_lsp::range(use_site.range(), &source, position_encoding);

        let span_path = world.path_for_id(id).ok()?;
        let span_source = world.source(id).ok()?;
        let offset = span_source.find(span)?;
        let typst_range = offset.range();
        let range = typst_to_lsp::range(typst_range, &span_source, position_encoding);

        let uri = Url::from_file_path(span_path).ok()?;

        let res = Some(GotoDefinitionResponse::Link(vec![LocationLink {
            origin_selection_range: Some(origin_selection_range),
            target_uri: uri,
            target_range: range,
            target_selection_range: range,
        }]));

        debug!("goto_definition: {res:?}");
        res
    }
}
