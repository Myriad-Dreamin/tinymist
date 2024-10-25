use once_cell::sync::OnceCell;
use typst_shim::syntax::LinkedNodeExt;

use crate::{
    adt::interner::Interned,
    prelude::*,
    syntax::{get_check_target, get_deref_target, CheckTarget, ParamTarget},
    DocTooltip, LspParamInfo, SemanticRequest,
};

/// The [`textDocument/signatureHelp`] request is sent from the client to the
/// server to request signature information at a given cursor position.
///
/// [`textDocument/signatureHelp`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_signatureHelp
#[derive(Debug, Clone)]
pub struct SignatureHelpRequest {
    /// The path of the document to get signature help for.
    pub path: PathBuf,
    /// The position of the cursor to get signature help for.
    pub position: LspPosition,
}

impl SemanticRequest for SignatureHelpRequest {
    type Response = SignatureHelp;

    fn request(self, ctx: &mut AnalysisContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let cursor = ctx.to_typst_pos(self.position, &source)? + 1;

        let ast_node = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
        let CheckTarget::Param {
            callee,
            target,
            is_set,
            ..
        } = get_check_target(ast_node)?
        else {
            return None;
        };

        let deref_target = get_deref_target(callee, cursor)?;

        let def_link = ctx.definition(&source, None, deref_target)?;

        let documentation = DocTooltip::get(ctx, &def_link)
            .as_deref()
            .map(markdown_docs);

        let Some(Value::Func(function)) = def_link.value() else {
            return None;
        };
        log::trace!("got function {function:?}");

        let mut function = &function;
        use typst::foundations::func::Repr;
        let mut param_shift = 0;
        while let Repr::With(inner) = function.inner() {
            param_shift += inner.1.items.iter().filter(|x| x.name.is_none()).count();
            function = &inner.0;
        }

        let sig = ctx.signature_dyn(function.clone());

        log::debug!("got signature {sig:?}");

        let mut active_parameter = None;

        let mut label = def_link.name().as_ref().to_owned();
        let mut params = Vec::new();

        label.push('(');

        let mut real_offset = 0;
        let focus_name = OnceCell::new();
        for (i, (param, ty)) in sig.params().enumerate() {
            if is_set && !param.attrs.settable {
                continue;
            }

            match &target {
                ParamTarget::Positional { .. } if is_set => {}
                ParamTarget::Positional { positional, .. } => {
                    if (*positional) + param_shift == i {
                        active_parameter = Some(real_offset);
                    }
                }
                ParamTarget::Named(name) => {
                    let focus_name = focus_name
                        .get_or_init(|| Interned::new_str(&name.get().clone().into_text()));
                    if focus_name == &param.name {
                        active_parameter = Some(real_offset);
                    }
                }
            }

            real_offset += 1;

            if !params.is_empty() {
                label.push_str(", ");
            }

            label.push_str(&format!(
                "{}: {}",
                param.name,
                ty.unwrap_or(&param.ty)
                    .describe()
                    .as_deref()
                    .unwrap_or("any")
            ));

            params.push(LspParamInfo {
                label: lsp_types::ParameterLabel::Simple(format!("{}:", param.name)),
                documentation: param.docs.as_ref().map(|docs| {
                    Documentation::MarkupContent(MarkupContent {
                        value: docs.as_ref().into(),
                        kind: MarkupKind::Markdown,
                    })
                }),
            });
        }
        label.push(')');
        let ret = sig.type_sig().body.clone();
        if let Some(ret_ty) = ret {
            label.push_str(" -> ");
            label.push_str(ret_ty.describe().as_deref().unwrap_or("any"));
        }

        if matches!(target, ParamTarget::Positional { .. }) {
            active_parameter =
                active_parameter.map(|x| x.min(sig.primary().pos_size().saturating_sub(1)));
        }

        log::trace!("got signature info {label} {params:?}");

        Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label: label.to_string(),
                documentation,
                parameters: Some(params),
                active_parameter: active_parameter.map(|x| x as u32),
            }],
            active_signature: Some(0),
            active_parameter: None,
        })
    }
}

fn markdown_docs(docs: &str) -> Documentation {
    Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: docs.to_owned(),
    })
}
