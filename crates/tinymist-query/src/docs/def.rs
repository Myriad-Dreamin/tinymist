use std::sync::{Arc, OnceLock};

use ecow::EcoString;
use tinymist_analysis::Signature;
use tinymist_analysis::docs::tidy::remove_list_annotations;
use tinymist_analysis::docs::{
    DocText, DocTextResolver, ParamDocs, SignatureDocs, VarDocs, format_ty_short,
};
use tinymist_analysis::ty::DocSource;
use typst::syntax::Span;

use crate::analysis::SharedContext;

pub(crate) fn var_docs(ctx: &Arc<SharedContext>, pos: Span) -> Option<VarDocs> {
    let source = ctx.source_by_id(pos.id()?).ok()?;
    let type_info = ctx.type_check(&source);
    let ty = type_info.type_of_span(pos)?;

    // todo multiple sources
    // Must use raw result as type aliases contain the source information.
    let mut srcs = ty.sources();
    srcs.sort();
    log::debug!("check variable docs of ty: {ty:?} => {srcs:?}");
    let doc_source = srcs.into_iter().next()?;

    // todo people can easily forget to simplify the type which is not good. we
    // might find a way to ensure them at compile time.
    //
    // Must be simplified before formatting, to expand type aliases.
    let simplified_ty = type_info.simplify(ty, false);
    let return_ty = format_ty_short(Some(&simplified_ty));
    match doc_source {
        DocSource::Var(var) => {
            let docs = type_info
                .var_docs
                .get(&var.def)
                .map(|docstring| docstring.docs().clone());
            Some(VarDocs {
                docs: docs.unwrap_or_default(),
                return_ty,
                def_docs: OnceLock::new(),
            })
        }
        DocSource::Ins(ins) => ins.syntax.as_ref().map(|src| {
            let docs = convert_typst_docs_shared(ctx, src.doc.as_ref().into());
            VarDocs {
                docs,
                return_ty,
                def_docs: OnceLock::new(),
            }
        }),
        _ => None,
    }
}

pub(crate) fn sig_docs(ctx: &Arc<SharedContext>, sig: &Signature) -> Option<SignatureDocs> {
    let type_sig = sig.type_sig().clone();
    let mut resolver = ctx.as_ref();

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
        .map(|(param, ty)| ParamDocs::new(&mut resolver, param, ty))
        .collect::<Vec<_>>();
    let named = named_in
        .map(|(param, ty)| (param.name.clone(), ParamDocs::new(&mut resolver, param, ty)))
        .collect::<std::collections::BTreeMap<_, _>>();
    let rest = rest_in.map(|(param, ty)| ParamDocs::new(&mut resolver, param, ty));

    let ret_ty = format_ty_short(ret_in);

    let docs = sig
        .primary()
        .docs
        .as_ref()
        .map(|docs| resolve_doc_text(ctx, docs))
        .unwrap_or_default();

    Some(SignatureDocs {
        docs,
        pos,
        named,
        rest,
        ret_ty,
        hover_docs: OnceLock::new(),
    })
}

pub(crate) fn resolve_doc_text(ctx: &SharedContext, docs: &DocText) -> EcoString {
    docs.get_or_init(|raw| convert_official_doc(ctx, raw.clone()))
        .clone()
}

impl DocTextResolver for &SharedContext {
    fn resolve_doc_text(&mut self, docs: &DocText) -> EcoString {
        resolve_doc_text(self, docs)
    }
}

fn convert_official_doc(ctx: &SharedContext, docs: EcoString) -> EcoString {
    if docs.trim().is_empty() {
        return docs;
    }

    let docs_text = DocText::official(docs.clone());
    match crate::docs::convert_docs(ctx, &docs_text, None) {
        Ok(converted) => {
            let converted = remove_list_annotations(&converted);
            ctx.remove_html(converted.trim().into())
        }
        Err(err) => {
            log::warn!("failed to convert official Typst docs to Markdown: {err}");
            ctx.remove_html(docs)
        }
    }
}

pub(crate) fn convert_typst_docs_shared(ctx: &SharedContext, docs: EcoString) -> EcoString {
    if docs.trim().is_empty() {
        return docs;
    }

    let docs_text = DocText::plain(docs.clone());
    match crate::docs::convert_docs(ctx, &docs_text, None) {
        Ok(converted) => {
            let converted = remove_list_annotations(&converted);
            ctx.remove_html(converted.trim().into())
        }
        Err(err) => {
            log::warn!("failed to convert Typst docs to Markdown: {err}");
            ctx.remove_html(docs)
        }
    }
}
