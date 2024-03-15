use log::debug;

use crate::{
    analysis::{get_def_use, get_deref_target, DerefTarget, IdentRef},
    prelude::*,
};

#[derive(Debug, Clone)]
pub struct ReferencesRequest {
    pub path: PathBuf,
    pub position: LspPosition,
}

impl ReferencesRequest {
    pub fn request(
        self,
        ctx: &mut AnalysisContext,
        position_encoding: PositionEncoding,
    ) -> Option<Vec<LspLocation>> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let offset = lsp_to_typst::position(self.position, position_encoding, &source)?;
        let cursor = offset + 1;

        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        debug!("ast_node: {ast_node:?}", ast_node = ast_node);
        let deref_target = get_deref_target(ast_node)?;

        let def_use = get_def_use(ctx, source.clone())?;
        let locations = find_references(ctx, def_use, deref_target, position_encoding)?;

        debug!("references: {locations:?}");
        Some(locations)
    }
}

fn find_references(
    ctx: &mut AnalysisContext<'_>,
    def_use: Arc<crate::analysis::DefUseInfo>,
    deref_target: DerefTarget<'_>,
    position_encoding: PositionEncoding,
) -> Option<Vec<LspLocation>> {
    let node = match deref_target {
        DerefTarget::VarAccess(node) => node,
        DerefTarget::Callee(node) => node,
        DerefTarget::ImportPath(..) => {
            return None;
        }
    };

    let mut may_ident = node.cast::<ast::Expr>()?;
    let name;
    loop {
        match may_ident {
            ast::Expr::Parenthesized(e) => {
                may_ident = e.expr();
            }
            ast::Expr::FieldAccess(e) => {
                may_ident = e.target();
            }
            ast::Expr::MathIdent(e) => {
                name = e.get().to_string();
                break;
            }
            ast::Expr::Ident(e) => {
                name = e.get().to_string();
                break;
            }
            _ => return None,
        }
    }

    let ident = node.find(may_ident.span())?;

    // todo: if it is exported, find all the references in the workspace
    let ident_ref = IdentRef {
        name,
        range: ident.range(),
    };
    let def_fid = ident.span().id()?;

    let (id, _) = def_use.get_def(def_fid, &ident_ref)?;
    let def_source = ctx.source_by_id(def_fid).ok()?;

    let def_path = ctx.world.path_for_id(def_fid).ok()?;
    let uri = Url::from_file_path(def_path).ok()?;

    let mut references = def_use
        .get_refs(id)
        .map(|r| {
            let range = typst_to_lsp::range(r.range.clone(), &def_source, position_encoding);

            LspLocation {
                uri: uri.clone(),
                range,
            }
        })
        .collect::<Vec<_>>();

    if def_use.is_exported(id) {
        // Find dependents
        let mut ctx = ctx.fork_for_search();
        ctx.push_dependents(def_fid);
        while let Some(ref_fid) = ctx.worklist.pop() {
            let ref_source = ctx.ctx.source_by_id(ref_fid).ok()?;
            let def_use = get_def_use(ctx.ctx, ref_source.clone())?;
            let (id, _) = def_use.get_def(def_fid, &ident_ref)?;

            let uri = ctx.ctx.world.path_for_id(ref_fid).ok()?;
            let uri = Url::from_file_path(uri).ok()?;
            let locations = def_use.get_refs(id).map(|r| {
                let range = typst_to_lsp::range(r.range.clone(), &ref_source, position_encoding);

                LspLocation {
                    uri: uri.clone(),
                    range,
                }
            });
            references.extend(locations);

            if def_use.is_exported(id) {
                ctx.push_dependents(ref_fid);
            }
        }
    }

    Some(references)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        // goto_definition
        snapshot_testing2("references", &|world, path| {
            let source = world.source_by_path(&path).unwrap();

            let request = ReferencesRequest {
                path: path.clone(),
                position: find_test_position(&source),
            };

            let result = request.request(world, PositionEncoding::Utf16);
            // sort
            let result = result.map(|mut e| {
                e.sort_by(|a, b| match a.range.start.cmp(&b.range.start) {
                    std::cmp::Ordering::Equal => a.range.end.cmp(&b.range.end),
                    e => e,
                });
                e
            });
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
