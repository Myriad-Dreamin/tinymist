use log::debug;
use tower_lsp::lsp_types::LocationLink;

use crate::prelude::*;

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
        let typst_offset =
            lsp_to_typst::position_to_offset(self.position, position_encoding, &source);

        let ast_node = LinkedNode::new(source.root()).leaf_at(typst_offset)?;

        let mut ancestor = &ast_node;
        while !ancestor.is::<ast::Expr>() {
            ancestor = ancestor.parent()?;
        }

        let may_ident = ancestor.cast::<ast::Expr>()?;
        if !may_ident.hash() && !matches!(may_ident, ast::Expr::MathIdent(_)) {
            return None;
        }

        let mut is_ident_only = false;
        trace!("got ast_node kind {kind:?}", kind = ancestor.kind());
        let callee_node = match may_ident {
            // todo: label, reference
            // todo: import
            // todo: include
            ast::Expr::FuncCall(call) => call.callee(),
            ast::Expr::Set(set) => set.target(),
            ast::Expr::Ident(..) | ast::Expr::MathIdent(..) | ast::Expr::FieldAccess(..) => {
                is_ident_only = true;
                may_ident
            }
            _ => return None,
        };
        trace!("got callee_node {callee_node:?} {is_ident_only:?}");

        let callee_link = if is_ident_only {
            ancestor.clone()
        } else {
            ancestor.find(callee_node.span())?
        };

        let values = analyze_expr(world, &callee_link);

        let func_or_module = values.into_iter().find_map(|v| match &v {
            Value::Args(a) => {
                trace!("got args {a:?}");
                None
            }
            Value::Func(..) | Value::Module(..) => Some(v),
            _ => None,
        });

        let span = match func_or_module {
            Some(Value::Func(f)) => f.span(),
            Some(Value::Module(m)) => {
                trace!("find module. {m:?}");
                // todo
                return None;
            }
            _ => {
                trace!("find value by lexical result. {callee_link:?}");
                return None;
            }
        };

        if span.is_detached() {
            return None;
        }

        let Some(id) = span.id() else {
            return None;
        };

        let origin_selection_range =
            typst_to_lsp::range(callee_link.range(), &source, position_encoding).raw_range;

        let span_path = world.path_for_id(id).ok()?;
        let span_source = world.source(id).ok()?;
        let offset = span_source.find(span)?;
        let typst_range = offset.range();
        let range = typst_to_lsp::range(typst_range, &span_source, position_encoding).raw_range;

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
