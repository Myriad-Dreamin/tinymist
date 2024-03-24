use crate::{prelude::*, SyntaxRequest};

#[derive(Debug, Clone)]
pub struct SignatureHelpRequest {
    pub path: PathBuf,
    pub position: LspPosition,
}

impl SyntaxRequest for SignatureHelpRequest {
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

fn param_index_at_leaf(leaf: &LinkedNode, function: &Func, args: ast::Args) -> Option<usize> {
    let deciding = deciding_syntax(leaf);
    let params = function.params()?;
    let param_index = find_param_index(&deciding, params, args)?;
    trace!("got param index {param_index}");
    Some(param_index)
}

/// Find the piece of syntax that decides what we're completing.
fn deciding_syntax<'b>(leaf: &'b LinkedNode) -> LinkedNode<'b> {
    let mut deciding = leaf.clone();
    while !matches!(
        deciding.kind(),
        SyntaxKind::LeftParen | SyntaxKind::Comma | SyntaxKind::Colon
    ) {
        let Some(prev) = deciding.prev_leaf() else {
            break;
        };
        deciding = prev;
    }
    deciding
}

fn find_param_index(deciding: &LinkedNode, params: &[ParamInfo], args: ast::Args) -> Option<usize> {
    match deciding.kind() {
        // After colon: "func(param:|)", "func(param: |)".
        SyntaxKind::Colon => {
            let prev = deciding.prev_leaf()?;
            let param_ident = prev.cast::<ast::Ident>()?;
            params
                .iter()
                .position(|param| param.name == param_ident.as_str())
        }
        // Before: "func(|)", "func(hi|)", "func(12,|)".
        SyntaxKind::Comma | SyntaxKind::LeftParen => {
            let next = deciding.next_leaf();
            let following_param = next.as_ref().and_then(|next| next.cast::<ast::Ident>());
            match following_param {
                Some(next) => params
                    .iter()
                    .position(|param| param.named && param.name.starts_with(next.as_str())),
                None => {
                    let positional_args_so_far = args
                        .items()
                        .filter(|arg| matches!(arg, ast::Arg::Pos(_)))
                        .count();
                    params
                        .iter()
                        .enumerate()
                        .filter(|(_, param)| param.positional)
                        .map(|(i, _)| i)
                        .nth(positional_args_so_far)
                }
            }
        }
        _ => None,
    }
}

fn markdown_docs(docs: &str) -> Documentation {
    Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: docs.to_owned(),
    })
}
