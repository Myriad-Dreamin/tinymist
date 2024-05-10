use crate::{
    analysis::{analyze_dyn_signature, find_definition, Ty},
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

        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        let CheckTarget::Param { callee, target, .. } = get_check_target(ast_node)? else {
            return None;
        };

        let deref_target = get_deref_target(callee, cursor)?;

        let def_link = find_definition(ctx, source.clone(), None, deref_target)?;

        let documentation = DocTooltip::get(ctx, &def_link)
            .as_deref()
            .map(markdown_docs);

        let Some(Value::Func(function)) = def_link.value else {
            return None;
        };
        trace!("got function {function:?}");

        let mut function = &function;
        use typst::foundations::func::Repr;
        let mut param_shift = 0;
        while let Repr::With(inner) = function.inner() {
            param_shift += inner.1.items.iter().filter(|x| x.name.is_none()).count();
            function = &inner.0;
        }

        let sig = analyze_dyn_signature(ctx, function.clone());
        let pos = &sig.primary().pos;
        let mut named = sig.primary().named.values().collect::<Vec<_>>();
        let rest = &sig.primary().rest;

        named.sort_by_key(|x| &x.name);

        let active_parameter = match &target {
            ParamTarget::Positional { positional, .. } => Some((*positional) + param_shift),
            ParamTarget::Named(name) => {
                let name = name.get().clone().into_text();
                named
                    .iter()
                    .position(|x| x.name.as_ref() == name.as_ref())
                    .map(|i| pos.len() + i)
            }
        };

        let mut label = def_link.name.clone();
        let mut params = Vec::new();

        label.push('(');
        for ty in pos.iter().chain(named.into_iter()).chain(rest.iter()) {
            if !params.is_empty() {
                label.push_str(", ");
            }

            label.push_str(&format!(
                "{}: {}",
                ty.name,
                ty.infer_type
                    .as_ref()
                    .unwrap_or(&Ty::Any)
                    .describe()
                    .as_deref()
                    .unwrap_or("any")
            ));

            params.push(LspParamInfo {
                label: lsp_types::ParameterLabel::Simple(ty.name.clone().into()),
                documentation: if !ty.docs.is_empty() {
                    Some(Documentation::MarkupContent(MarkupContent {
                        value: ty.docs.clone().into(),
                        kind: MarkupKind::Markdown,
                    }))
                } else {
                    None
                },
            });
        }
        label.push(')');
        if let Some(ret_ty) = sig.primary().ret_ty.as_ref() {
            label.push_str(" -> ");
            label.push_str(ret_ty.describe().as_deref().unwrap_or("any"));
        }

        trace!("got signature info {label} {params:?}");

        Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label,
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
