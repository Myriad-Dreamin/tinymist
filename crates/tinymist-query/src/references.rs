use std::ops::Range;

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
        ctx: &TypstSystemWorld,
        position_encoding: PositionEncoding,
    ) -> Option<Vec<LspLocation>> {
        let mut ctx = AnalysisContext::new(ctx);

        let world = ctx.world;
        let source = ctx.source_by_path(&self.path).ok()?;
        let offset = lsp_to_typst::position(self.position, position_encoding, &source)?;
        let cursor = offset + 1;

        let w: &dyn World = world;
        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        debug!("ast_node: {ast_node:?}", ast_node = ast_node);
        let deref_target = get_deref_target(ast_node)?;

        let def_use = get_def_use(&mut ctx, source.clone())?;
        let ref_spans = find_declarations(w, def_use, deref_target)?;

        let mut locations = vec![];
        for ref_range in ref_spans {
            let ref_id = source.id();
            let ref_source = &source;

            let span_path = world.path_for_id(ref_id).ok()?;
            let range = typst_to_lsp::range(ref_range, ref_source, position_encoding);

            let uri = Url::from_file_path(span_path).ok()?;

            locations.push(LspLocation { uri, range });
        }

        debug!("references: {locations:?}");
        Some(locations)
    }
}

fn find_declarations(
    _w: &dyn World,
    def_use: Arc<crate::analysis::DefUseInfo>,
    deref_target: DerefTarget<'_>,
) -> Option<Vec<Range<usize>>> {
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

    let (id, _) = def_use.get_def(ident.span().id()?, &ident_ref)?;
    Some(
        def_use
            .get_refs(id)
            .map(|r| r.range.clone())
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        // goto_definition
        snapshot_testing("references", &|world, path| {
            let source = get_suitable_source_in_workspace(world, &path).unwrap();

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
