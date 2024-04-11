use crate::{prelude::*, syntax::param_index_at_leaf, SemanticRequest};

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
        let typst_offset = ctx.to_typst_pos(self.position, &source)?;

        let ast_node = LinkedNode::new(source.root()).leaf_at(typst_offset + 1)?;
        let (callee, callee_node, args) = surrounding_function_syntax(&ast_node)?;

        if !callee.hash() && !matches!(callee, ast::Expr::MathIdent(_)) {
            return None;
        }

        let values = analyze_expr(ctx.world(), &callee_node);

        let function = values.into_iter().find_map(|v| match v.0 {
            Value::Func(f) => Some(f),
            _ => None,
        })?;
        trace!("got function {function:?}");

        let param_index = param_index_at_leaf(&ast_node, &function, args);

        let label = format!(
            "{}({}){}",
            function.name().unwrap_or("<anonymous closure>"),
            match function.params() {
                Some(params) => params
                    .iter()
                    .map(typst_to_lsp::param_info_to_label)
                    .join(", "),
                None => "".to_owned(),
            },
            match function.returns() {
                Some(returns) => format!("-> {}", typst_to_lsp::cast_info_to_label(returns)),
                None => "".to_owned(),
            }
        );
        let params = function
            .params()
            .unwrap_or_default()
            .iter()
            .map(typst_to_lsp::param_info)
            .collect();
        trace!("got signature info {label} {params:?}");

        let documentation = function.docs().map(markdown_docs);

        let active_parameter = param_index.map(|i| i as u32);

        Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label,
                documentation,
                parameters: Some(params),
                active_parameter,
            }],
            active_signature: Some(0),
            active_parameter: None,
        })
    }
}

fn surrounding_function_syntax<'b>(
    leaf: &'b LinkedNode,
) -> Option<(ast::Expr<'b>, LinkedNode<'b>, ast::Args<'b>)> {
    let parent = leaf.parent()?;
    let parent = match parent.kind() {
        SyntaxKind::Named => parent.parent()?,
        _ => parent,
    };
    let args = parent.cast::<ast::Args>()?;
    let grand = parent.parent()?;
    let expr = grand.cast::<ast::Expr>()?;
    let callee = match expr {
        ast::Expr::FuncCall(call) => call.callee(),
        ast::Expr::Set(set) => set.target(),
        _ => return None,
    };
    Some((callee, grand.find(callee.span())?, args))
}

fn markdown_docs(docs: &str) -> Documentation {
    Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: docs.to_owned(),
    })
}
