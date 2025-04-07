use std::sync::OnceLock;

use tinymist_analysis::docs::{format_ty, ParamDocs, SignatureDocs, VarDocs};
use tinymist_analysis::ty::DocSource;
use tinymist_analysis::Signature;
use typst::syntax::Span;

use crate::LocalContext;

pub(crate) fn var_docs(ctx: &mut LocalContext, pos: Span) -> Option<VarDocs> {
    let source = ctx.source_by_id(pos.id()?).ok()?;
    let type_info = ctx.type_check(&source);
    let ty = type_info.type_of_span(pos)?;

    // todo multiple sources
    // Must use raw result as type aliases contain the source information.
    let mut srcs = ty.sources();
    srcs.sort();
    log::info!("check variable docs of ty: {ty:?} => {srcs:?}");
    let doc_source = srcs.into_iter().next()?;

    // todo people can easily forget to simplify the type which is not good. we
    // might find a way to ensure them at compile time.
    //
    // Must be simplified before formatting, to expand type aliases.
    let simplified_ty = type_info.simplify(ty, false);
    let return_ty = format_ty(Some(&simplified_ty));
    match doc_source {
        DocSource::Var(var) => {
            let docs = type_info
                .var_docs
                .get(&var.def)
                .map(|docs| docs.docs().clone());
            Some(VarDocs {
                docs: docs.unwrap_or_default(),
                return_ty,
                def_docs: OnceLock::new(),
            })
        }
        DocSource::Ins(ins) => ins.syntax.as_ref().map(|src| {
            let docs = src.doc.as_ref().into();
            VarDocs {
                docs,
                return_ty,
                def_docs: OnceLock::new(),
            }
        }),
        _ => None,
    }
}

pub(crate) fn sig_docs(sig: &Signature) -> Option<SignatureDocs> {
    let type_sig = sig.type_sig().clone();

    let pos_in = sig
        .primary()
        .pos()
        .iter()
        .enumerate()
        .map(|(idx, pos)| (pos, type_sig.pos(idx)));
    let named_in = sig
        .primary()
        .named()
        .iter()
        .map(|param| (param, type_sig.named(&param.name)));
    let rest_in = sig.primary().rest().map(|x| (x, type_sig.rest_param()));

    let ret_in = type_sig.body.as_ref();

    let pos = pos_in
        .map(|(param, ty)| ParamDocs::new(param, ty))
        .collect();
    let named = named_in
        .map(|(param, ty)| (param.name.clone(), ParamDocs::new(param, ty)))
        .collect();
    let rest = rest_in.map(|(param, ty)| ParamDocs::new(param, ty));

    let ret_ty = format_ty(ret_in);

    Some(SignatureDocs {
        docs: sig.primary().docs.clone().unwrap_or_default(),
        pos,
        named,
        rest,
        ret_ty,
        hover_docs: OnceLock::new(),
    })
}
