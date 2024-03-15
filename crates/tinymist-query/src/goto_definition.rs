use std::ops::Range;

use log::debug;
use typst::foundations::Value;
use typst_ts_core::TypstFileId;

use crate::{
    prelude::*,
    syntax::{
        find_source_by_import, get_deref_target, DerefTarget, IdentRef, LexicalKind,
        LexicalModKind, LexicalVarKind,
    },
};

#[derive(Debug, Clone)]
pub struct GotoDefinitionRequest {
    pub path: PathBuf,
    pub position: LspPosition,
}

impl GotoDefinitionRequest {
    pub fn request(
        self,
        ctx: &mut AnalysisContext,
        position_encoding: PositionEncoding,
    ) -> Option<GotoDefinitionResponse> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let offset = lsp_to_typst::position(self.position, position_encoding, &source)?;
        let cursor = offset + 1;

        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        debug!("ast_node: {ast_node:?}", ast_node = ast_node);

        let deref_target = get_deref_target(ast_node)?;
        let use_site = deref_target.node().clone();
        let origin_selection_range =
            typst_to_lsp::range(use_site.range(), &source, position_encoding);

        let def = find_definition(ctx, source.clone(), deref_target)?;

        let span_path = ctx.world.path_for_id(def.fid).ok()?;
        let uri = Url::from_file_path(span_path).ok()?;

        let span_source = ctx.source_by_id(def.fid).ok()?;
        let range = typst_to_lsp::range(def.def_range, &span_source, position_encoding);

        let res = Some(GotoDefinitionResponse::Link(vec![LocationLink {
            origin_selection_range: Some(origin_selection_range),
            target_uri: uri,
            target_range: range,
            target_selection_range: range,
        }]));

        debug!("goto_definition: {:?} {res:?}", def.fid);
        res
    }
}

pub(crate) struct DefinitionLink {
    pub value: Option<Value>,
    pub fid: TypstFileId,
    pub name: String,
    pub def_range: Range<usize>,
    pub name_range: Option<Range<usize>>,
}

// todo: field definition
pub(crate) fn find_definition(
    ctx: &mut AnalysisContext<'_>,
    source: Source,
    deref_target: DerefTarget<'_>,
) -> Option<DefinitionLink> {
    let source_id = source.id();

    let use_site = match deref_target {
        // todi: field access
        DerefTarget::VarAccess(node) | DerefTarget::Callee(node) => node,
        // todo: better support (rename import path?)
        DerefTarget::ImportPath(path) => {
            let parent = path.parent()?;
            let def_fid = parent.span().id()?;
            let e = parent.cast::<ast::ModuleImport>()?;
            let source = find_source_by_import(ctx.world, def_fid, e)?;
            return Some(DefinitionLink {
                name: String::new(),
                value: None,
                fid: source.id(),
                def_range: (LinkedNode::new(source.root())).range(),
                name_range: None,
            });
        }
    };

    // syntatic definition
    let def_use = ctx.def_use(source)?;
    let ident_ref = match use_site.cast::<ast::Expr>()? {
        ast::Expr::Ident(e) => IdentRef {
            name: e.get().to_string(),
            range: use_site.range(),
        },
        ast::Expr::MathIdent(e) => IdentRef {
            name: e.get().to_string(),
            range: use_site.range(),
        },
        ast::Expr::FieldAccess(..) => {
            debug!("find field access");
            return None;
        }
        _ => {
            debug!("unsupported kind {kind:?}", kind = use_site.kind());
            return None;
        }
    };
    let def_id = def_use.get_ref(&ident_ref);
    let def_id = def_id.or_else(|| Some(def_use.get_def(source_id, &ident_ref)?.0));
    let def_info = def_id.and_then(|def_id| def_use.get_def_by_id(def_id));

    let values = analyze_expr(ctx.world, &use_site);
    for v in values {
        // mostly builtin functions
        if let Value::Func(f) = v.0 {
            use typst::foundations::func::Repr;
            match f.inner() {
                // The with function should be resolved as the with position
                Repr::Closure(..) | Repr::With(..) => continue,
                Repr::Native(..) | Repr::Element(..) => {}
            }

            let name = f
                .name()
                .or_else(|| def_info.as_ref().map(|(_, r)| r.name.as_str()));

            if let Some(name) = name {
                let span = f.span();
                let fid = span.id()?;
                let source = ctx.source_by_id(fid).ok()?;

                return Some(DefinitionLink {
                    name: name.to_owned(),
                    value: Some(Value::Func(f.clone())),
                    fid,
                    def_range: source.find(span)?.range(),
                    name_range: def_info.map(|(_, r)| r.range.clone()),
                });
            }
        }
    }

    let (def_fid, def) = def_info?;

    match def.kind {
        LexicalKind::Heading(..) | LexicalKind::Block => unreachable!(),
        LexicalKind::Var(
            LexicalVarKind::Variable
            | LexicalVarKind::ValRef
            | LexicalVarKind::Label
            | LexicalVarKind::LabelRef,
        )
        | LexicalKind::Mod(
            LexicalModKind::Module(..)
            | LexicalModKind::PathVar
            | LexicalModKind::ModuleAlias
            | LexicalModKind::Alias { .. }
            | LexicalModKind::Ident,
        ) => Some(DefinitionLink {
            name: def.name.clone(),
            value: None,
            fid: def_fid,
            def_range: def.range.clone(),
            name_range: Some(def.range.clone()),
        }),
        LexicalKind::Var(LexicalVarKind::Function) => {
            let def_source = ctx.source_by_id(def_fid).ok()?;
            let root = LinkedNode::new(def_source.root());
            let def_name = root.leaf_at(def.range.start + 1)?;
            log::info!("def_name for function: {def_name:?}", def_name = def_name);
            let values = analyze_expr(ctx.world, &def_name);
            let Some(func) = values.into_iter().find_map(|v| match v.0 {
                Value::Func(f) => Some(f),
                _ => None,
            }) else {
                log::info!("no func found... {:?}", def.name);
                return None;
            };
            log::info!("okay for function: {func:?}");

            Some(DefinitionLink {
                name: def.name.clone(),
                value: Some(Value::Func(func.clone())),
                fid: def_fid,
                def_range: def.range.clone(),
                name_range: Some(def.range.clone()),
            })
        }
        LexicalKind::Mod(LexicalModKind::Star) => {
            log::info!("unimplemented star import {:?}", ident_ref);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing2("goto_definition", &|world, path| {
            let source = world.source_by_path(&path).unwrap();

            let request = GotoDefinitionRequest {
                path: path.clone(),
                position: find_test_position(&source),
            };

            let result = request.request(world, PositionEncoding::Utf16);
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
