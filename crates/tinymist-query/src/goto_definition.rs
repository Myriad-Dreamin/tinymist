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
        let offset = lsp_to_typst::position(self.position, position_encoding, &source)?;
        let cursor = offset + 1;

        let def = {
            let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
            let t: &dyn World = world;
            find_definition(t.track(), source.id(), ast_node)?
        };
        let (span, use_site) = (def.span(), def.use_site());

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
        let def_node = span_source.find(span)?;
        let typst_range = def_node.range();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        // goto_definition
        snapshot_testing("goto_definition", &|world, path| {
            let source = get_suitable_source_in_workspace(world, &path).unwrap();

            let request = GotoDefinitionRequest {
                path: path.clone(),
                position: find_test_position(&source),
            };

            let result = request.request(world, PositionEncoding::Utf16);
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
