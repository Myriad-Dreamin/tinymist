use std::ops::Range;

use log::debug;
use lsp_types::LocationLink;

use crate::{
    prelude::*,
    syntax::{get_deref_target, DerefTarget},
};

#[derive(Debug, Clone)]
pub struct GotoDeclarationRequest {
    pub path: PathBuf,
    pub position: LspPosition,
}

impl GotoDeclarationRequest {
    pub fn request(
        self,
        world: &TypstSystemWorld,
        position_encoding: PositionEncoding,
    ) -> Option<GotoDeclarationResponse> {
        let mut ctx = AnalysisContext::new(world, position_encoding);
        let source = get_suitable_source_in_workspace(world, &self.path).ok()?;
        let offset = lsp_to_typst::position(self.position, position_encoding, &source)?;
        let cursor = offset + 1;

        let w: &dyn World = world;
        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        debug!("ast_node: {ast_node:?}", ast_node = ast_node);
        let deref_target = get_deref_target(ast_node)?;

        let use_site = deref_target.node();
        let origin_selection_range =
            typst_to_lsp::range(use_site.range(), &source, position_encoding);

        let def_use = ctx.def_use(source.clone())?;
        let ref_spans = find_declarations(w, def_use, deref_target)?;

        let mut links = vec![];
        for ref_range in ref_spans {
            let ref_id = source.id();
            let ref_source = &source;

            let span_path = world.path_for_id(ref_id).ok()?;
            let range = typst_to_lsp::range(ref_range, ref_source, position_encoding);

            let uri = Url::from_file_path(span_path).ok()?;

            links.push(LocationLink {
                origin_selection_range: Some(origin_selection_range),
                target_uri: uri,
                target_range: range,
                target_selection_range: range,
            });
        }

        debug!("goto_declartion: {links:?}");
        Some(GotoDeclarationResponse::Link(links))
    }
}

fn find_declarations(
    _w: &dyn World,
    _def_use: Arc<crate::analysis::DefUseInfo>,
    _deref_target: DerefTarget<'_>,
) -> Option<Vec<Range<usize>>> {
    todo!()
}
