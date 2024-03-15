use log::debug;
use lsp_types::TextEdit;

use crate::{
    find_definition, find_references, prelude::*, syntax::get_deref_target,
    validate_renaming_definition,
};

#[derive(Debug, Clone)]
pub struct RenameRequest {
    pub path: PathBuf,
    pub position: LspPosition,
    pub new_name: String,
}

impl RenameRequest {
    pub fn request(
        self,
        ctx: &mut AnalysisContext,
        position_encoding: PositionEncoding,
    ) -> Option<WorkspaceEdit> {
        let source = ctx.source_by_path(&self.path).ok()?;

        let offset = lsp_to_typst::position(self.position, position_encoding, &source)?;
        let cursor = offset + 1;

        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        debug!("ast_node: {ast_node:?}", ast_node = ast_node);

        let deref_target = get_deref_target(ast_node)?;

        let lnk = find_definition(ctx, source.clone(), deref_target.clone())?;

        validate_renaming_definition(&lnk)?;

        let def_use = ctx.def_use(source.clone())?;
        let references = find_references(ctx, def_use, deref_target, position_encoding)?;

        let mut editions = HashMap::new();

        let def_loc = {
            let def_source = ctx.source_by_id(lnk.fid).ok()?;

            let span_path = ctx.world.path_for_id(lnk.fid).ok()?;
            let uri = Url::from_file_path(span_path).ok()?;

            let Some(range) = lnk.name_range else {
                log::warn!("rename: no name range");
                return None;
            };

            LspLocation {
                uri,
                range: typst_to_lsp::range(range, &def_source, position_encoding),
            }
        };

        for i in (Some(def_loc).into_iter()).chain(references) {
            let uri = i.uri;
            let range = i.range;
            let edits = editions.entry(uri).or_insert_with(Vec::new);
            edits.push(TextEdit {
                range,
                new_text: self.new_name.clone(),
            });
        }

        // todo: conflict analysis
        Some(WorkspaceEdit {
            changes: Some(editions),
            ..Default::default()
        })
    }
}
