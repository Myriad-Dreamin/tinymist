use std::ops::Range;

use comemo::Track;
use log::debug;
use lsp_types::LocationLink;

use crate::{
    analysis::{deref_lvalue, get_def_use, IdentRef},
    prelude::*,
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

        let def_use = get_def_use(w.track(), source.clone())?;
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

enum DerefTarget<'a> {
    VarAccess(LinkedNode<'a>),
    Callee(LinkedNode<'a>),
    ImportPath(LinkedNode<'a>),
}

impl<'a> DerefTarget<'a> {
    fn node(&self) -> &LinkedNode {
        match self {
            DerefTarget::VarAccess(node) => node,
            DerefTarget::Callee(node) => node,
            DerefTarget::ImportPath(node) => node,
        }
    }
}

fn get_deref_target(node: LinkedNode) -> Option<DerefTarget> {
    let mut ancestor = node;
    while !ancestor.is::<ast::Expr>() {
        ancestor = ancestor.parent()?.clone();
    }
    debug!("deref expr: {ancestor:?}");
    let ancestor = deref_lvalue(ancestor)?;
    debug!("deref lvalue: {ancestor:?}");

    let may_ident = ancestor.cast::<ast::Expr>()?;
    if !may_ident.hash() && !matches!(may_ident, ast::Expr::MathIdent(_)) {
        return None;
    }

    trace!("got ast_node kind {kind:?}", kind = ancestor.kind());

    Some(match may_ident {
        // todo: label, reference
        // todo: import
        // todo: include
        ast::Expr::FuncCall(call) => DerefTarget::Callee(ancestor.find(call.callee().span())?),
        ast::Expr::Set(set) => DerefTarget::Callee(ancestor.find(set.target().span())?),
        ast::Expr::Ident(..) | ast::Expr::MathIdent(..) | ast::Expr::FieldAccess(..) => {
            DerefTarget::VarAccess(ancestor.find(may_ident.span())?)
        }
        ast::Expr::Str(..) => {
            let parent = ancestor.parent()?;
            if parent.kind() != SyntaxKind::ModuleImport {
                return None;
            }

            return Some(DerefTarget::ImportPath(ancestor.find(may_ident.span())?));
        }
        ast::Expr::Import(..) => {
            return None;
        }
        _ => {
            debug!("unsupported kind {kind:?}", kind = ancestor.kind());
            return None;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        // goto_definition
        snapshot_testing("goto_declaration", &|world, path| {
            let source = get_suitable_source_in_workspace(world, &path).unwrap();

            let request = GotoDeclarationRequest {
                path: path.clone(),
                position: find_test_position(&source),
            };

            let result = request.request(world, PositionEncoding::Utf16);
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
