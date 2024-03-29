use log::debug;
use lsp_types::TextEdit;

use crate::{
    find_definition, find_references, prelude::*, syntax::get_deref_target,
    validate_renaming_definition, SemanticRequest,
};

/// The [`textDocument/rename`] request is sent from the client to the server to
/// ask the server to compute a workspace change so that the client can perform
/// a workspace-wide rename of a symbol.
///
/// [`textDocument/rename`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_rename
#[derive(Debug, Clone)]
pub struct RenameRequest {
    /// The path of the document to request for.
    pub path: PathBuf,
    /// The source code position to request for.
    pub position: LspPosition,
    /// The new name to rename to.
    pub new_name: String,
}

impl SemanticRequest for RenameRequest {
    type Response = WorkspaceEdit;

    fn request(self, ctx: &mut AnalysisContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;

        let offset = ctx.to_typst_pos(self.position, &source)?;
        let cursor = offset + 1;

        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        debug!("ast_node: {ast_node:?}", ast_node = ast_node);

        let deref_target = get_deref_target(ast_node, cursor)?;

        let lnk = find_definition(ctx, source.clone(), deref_target.clone())?;

        validate_renaming_definition(&lnk)?;

        let def_use = ctx.def_use(source.clone())?;
        let references = find_references(ctx, def_use, deref_target, ctx.position_encoding())?;

        let mut editions = HashMap::new();

        let def_loc = {
            let def_source = ctx.source_by_id(lnk.fid).ok()?;

            let span_path = ctx.path_for_id(lnk.fid).ok()?;
            let uri = path_to_url(&span_path).ok()?;

            let Some(range) = lnk.name_range else {
                log::warn!("rename: no name range");
                return None;
            };

            LspLocation {
                uri,
                range: ctx.to_lsp_range(range, &def_source),
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
