use log::debug;

use crate::{
    prelude::*,
    syntax::{get_deref_target, DerefTarget, IdentRef},
    SemanticRequest,
};

/// The [`textDocument/references`] request is sent from the client to the
/// server to resolve project-wide references for the symbol denoted by the
/// given text document position.
///
/// [`textDocument/references`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_references
#[derive(Debug, Clone)]
pub struct ReferencesRequest {
    /// The path of the document to request for.
    pub path: PathBuf,
    /// The source code position to request for.
    pub position: LspPosition,
}

impl SemanticRequest for ReferencesRequest {
    type Response = Vec<LspLocation>;

    fn request(self, ctx: &mut AnalysisContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let offset = ctx.to_typst_pos(self.position, &source)?;
        let cursor = offset + 1;

        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        debug!("ast_node: {ast_node:?}", ast_node = ast_node);
        let deref_target = get_deref_target(ast_node)?;

        let def_use = ctx.def_use(source.clone())?;
        let locations = find_references(ctx, def_use, deref_target, ctx.position_encoding())?;

        debug!("references: {locations:?}");
        Some(locations)
    }
}

pub(crate) fn find_references(
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
        name: name.clone(),
        range: ident.range(),
    };
    let cur_fid = ident.span().id()?;

    let def_id = def_use.get_ref(&ident_ref);
    let def_id = def_id.or_else(|| Some(def_use.get_def(cur_fid, &ident_ref)?.0));
    let (def_fid, def) = def_id.and_then(|def_id| def_use.get_def_by_id(def_id))?;
    let def_ident = IdentRef {
        name: def.name.clone(),
        range: def.range.clone(),
    };

    let def_source = ctx.source_by_id(def_fid).ok()?;
    let root_def_use = ctx.def_use(def_source)?;
    let root_def_id = root_def_use.get_def(def_fid, &def_ident)?.0;

    find_references_root(
        ctx,
        root_def_use,
        def_fid,
        root_def_id,
        def_ident,
        position_encoding,
    )
}

pub(crate) fn find_references_root(
    ctx: &mut AnalysisContext<'_>,
    def_use: Arc<crate::analysis::DefUseInfo>,
    def_fid: TypstFileId,
    def_id: DefId,
    def_ident: IdentRef,
    position_encoding: PositionEncoding,
) -> Option<Vec<LspLocation>> {
    let def_source = ctx.source_by_id(def_fid).ok()?;
    let def_path = ctx.path_for_id(def_fid).ok()?;
    let uri = Url::from_file_path(def_path).ok()?;

    // todo: reuse uri, range to location
    let mut references = def_use
        .get_refs(def_id)
        .map(|r| {
            let range = typst_to_lsp::range(r.range.clone(), &def_source, position_encoding);

            LspLocation {
                uri: uri.clone(),
                range,
            }
        })
        .collect::<Vec<_>>();

    if def_use.is_exported(def_id) {
        // Find dependents
        let mut ctx = ctx.fork_for_search();
        ctx.push_dependents(def_fid);
        while let Some(ref_fid) = ctx.worklist.pop() {
            let ref_source = ctx.ctx.source_by_id(ref_fid).ok()?;
            let def_use = ctx.ctx.def_use(ref_source.clone())?;

            let uri = ctx.ctx.path_for_id(ref_fid).ok()?;
            let uri = Url::from_file_path(uri).ok()?;

            let mut redefines = vec![];
            if let Some((id, _def)) = def_use.get_def(def_fid, &def_ident) {
                references.extend(def_use.get_refs(id).map(|r| {
                    let range =
                        typst_to_lsp::range(r.range.clone(), &ref_source, position_encoding);

                    LspLocation {
                        uri: uri.clone(),
                        range,
                    }
                }));
                redefines.push(id);

                if def_use.is_exported(id) {
                    ctx.push_dependents(ref_fid);
                }
            };
        }
    }

    Some(references)
}

#[cfg(test)]
mod tests {
    use typst_ts_core::path::unix_slash;

    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        // goto_definition
        snapshot_testing("references", &|world, path| {
            let source = world.source_by_path(&path).unwrap();

            let request = ReferencesRequest {
                path: path.clone(),
                position: find_test_position(&source),
            };

            let result = request.request(world);
            // sort
            let result = result.map(|mut e| {
                e.sort_by(|a, b| match a.range.start.cmp(&b.range.start) {
                    std::cmp::Ordering::Equal => a.range.end.cmp(&b.range.end),
                    e => e,
                });
                e
            });

            let result = result.map(|v| {
                v.into_iter()
                    .map(|l| {
                        let fp = unix_slash(&l.uri.to_file_path().unwrap());
                        let fp = fp.strip_prefix("C:").unwrap_or(&fp);
                        format!(
                            "{fp}@{}:{}:{}:{}",
                            l.range.start.line,
                            l.range.start.character,
                            l.range.end.line,
                            l.range.end.character
                        )
                    })
                    .collect::<Vec<_>>()
            });

            assert_snapshot!(JsonRepr::new_pure(result));
        });
    }
}
