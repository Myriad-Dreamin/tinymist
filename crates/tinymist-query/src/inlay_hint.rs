use std::ops::Range;

use log::debug;
use lsp_types::{InlayHintKind, InlayHintLabel};

use crate::{
    analysis::{analyze_call, ParamKind},
    prelude::*,
    SemanticRequest,
};

/// Configuration for inlay hints.
pub struct InlayHintConfig {
    // positional arguments group
    /// Show inlay hints for positional arguments.
    pub on_pos_args: bool,
    /// Disable inlay hints for single positional arguments.
    pub off_single_pos_arg: bool,

    // variadic arguments group
    /// Show inlay hints for variadic arguments.
    pub on_variadic_args: bool,
    /// Disable inlay hints for all variadic arguments but the first variadic
    /// argument.
    pub only_first_variadic_args: bool,

    // The typst sugar grammar
    /// Show inlay hints for content block arguments.
    pub on_content_block_args: bool,
}

impl InlayHintConfig {
    /// A smart configuration that enables most useful inlay hints.
    pub const fn smart() -> Self {
        Self {
            on_pos_args: true,
            off_single_pos_arg: true,

            on_variadic_args: true,
            only_first_variadic_args: true,

            on_content_block_args: false,
        }
    }
}

/// The [`textDocument/inlayHint`] request is sent from the client to the server
/// to compute inlay hints for a given `(text document, range)` tuple that may
/// be rendered in the editor in place with other text.
///
/// [`textDocument/inlayHint`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_inlayHint
///
/// # Compatibility
///
/// This request was introduced in specification version 3.17.0
#[derive(Debug, Clone)]
pub struct InlayHintRequest {
    /// The path of the document to get inlay hints for.
    pub path: PathBuf,
    /// The range of the document to get inlay hints for.
    pub range: LspRange,
}

impl SemanticRequest for InlayHintRequest {
    type Response = Vec<InlayHint>;

    fn request(self, ctx: &mut AnalysisContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let range = ctx.to_typst_range(self.range, &source)?;

        let hints = inlay_hint(ctx.world(), &source, range, ctx.position_encoding()).ok()?;
        debug!(
            "got inlay hints on {source:?} => {hints:?}",
            source = source.id(),
            hints = hints.len()
        );
        if hints.is_empty() {
            let root = LinkedNode::new(source.root());
            debug!("debug root {root:#?}");
        }

        Some(hints)
    }
}

fn inlay_hint(
    world: &dyn World,
    source: &Source,
    range: Range<usize>,
    encoding: PositionEncoding,
) -> FileResult<Vec<InlayHint>> {
    const SMART: InlayHintConfig = InlayHintConfig::smart();

    struct InlayHintWorker<'a> {
        world: &'a dyn World,
        source: &'a Source,
        range: Range<usize>,
        encoding: PositionEncoding,
        hints: Vec<InlayHint>,
    }

    impl InlayHintWorker<'_> {
        fn analyze(&mut self, node: LinkedNode) {
            let rng = node.range();
            if rng.start >= self.range.end || rng.end <= self.range.start {
                return;
            }

            self.analyze_node(&node);

            if node.get().children().len() == 0 {
                return;
            }

            // todo: survey bad performance children?
            for child in node.children() {
                self.analyze(child);
            }
        }

        fn analyze_node(&mut self, node: &LinkedNode) -> Option<()> {
            // analyze node self
            match node.kind() {
                // Type inlay hints
                SyntaxKind::LetBinding => {
                    trace!("let binding found: {:?}", node);
                }
                // Assignment inlay hints
                SyntaxKind::Eq => {
                    trace!("assignment found: {:?}", node);
                }
                SyntaxKind::DestructAssignment => {
                    trace!("destruct assignment found: {:?}", node);
                }
                // Parameter inlay hints
                SyntaxKind::FuncCall => {
                    trace!("func call found: {:?}", node);
                    let f = node.cast::<ast::FuncCall>().unwrap();

                    let callee = f.callee();
                    // todo: reduce many such patterns
                    if !callee.hash() && !matches!(callee, ast::Expr::MathIdent(_)) {
                        return None;
                    }

                    let callee_node = node.find(callee.span())?;

                    let args = f.args();
                    let args_node = node.find(args.span())?;

                    // todo: reduce many such patterns
                    let values = analyze_expr(self.world, &callee_node);
                    let func = values.into_iter().find_map(|v| match v.0 {
                        Value::Func(f) => Some(f),
                        _ => None,
                    })?;
                    log::debug!("got function {func:?}");

                    let call_info = analyze_call(func, args)?;
                    log::debug!("got call_info {call_info:?}");

                    let check_single_pos_arg = || {
                        let mut pos = 0;
                        let mut content_pos = 0;

                        for arg in args.items() {
                            let Some(arg_node) = args_node.find(arg.span()) else {
                                continue;
                            };

                            let Some(info) = call_info.arg_mapping.get(&arg_node) else {
                                continue;
                            };

                            if info.kind != ParamKind::Named {
                                if info.is_content_block {
                                    content_pos += 1;
                                } else {
                                    pos += 1;
                                };

                                if pos > 1 && content_pos > 1 {
                                    break;
                                }
                            }
                        }

                        (pos <= 1, content_pos <= 1)
                    };

                    let (disable_by_single_pos_arg, disable_by_single_content_pos_arg) =
                        if SMART.on_pos_args && SMART.off_single_pos_arg {
                            check_single_pos_arg()
                        } else {
                            (false, false)
                        };

                    let disable_by_single_line_content_block = !SMART.on_content_block_args
                        || 'one_line: {
                            for arg in args.items() {
                                let Some(arg_node) = args_node.find(arg.span()) else {
                                    continue;
                                };

                                let Some(info) = call_info.arg_mapping.get(&arg_node) else {
                                    continue;
                                };

                                if info.kind != ParamKind::Named
                                    && info.is_content_block
                                    && !is_one_line(self.source, &arg_node)
                                {
                                    break 'one_line false;
                                }
                            }

                            true
                        };

                    let mut is_first_variadic_arg = true;

                    for arg in args.items() {
                        let Some(arg_node) = args_node.find(arg.span()) else {
                            continue;
                        };

                        let Some(info) = call_info.arg_mapping.get(&arg_node) else {
                            continue;
                        };

                        if info.param.name.is_empty() {
                            continue;
                        }

                        match info.kind {
                            ParamKind::Named => {
                                continue;
                            }
                            ParamKind::Positional
                                if call_info.signature.has_fill_or_size_or_stroke =>
                            {
                                continue
                            }
                            ParamKind::Positional
                                if !SMART.on_pos_args
                                    || (info.is_content_block
                                        && (disable_by_single_content_pos_arg
                                            || disable_by_single_line_content_block))
                                    || (!info.is_content_block && disable_by_single_pos_arg) =>
                            {
                                continue
                            }
                            ParamKind::Rest
                                if (!SMART.on_variadic_args
                                    || (!is_first_variadic_arg
                                        && SMART.only_first_variadic_args)) =>
                            {
                                continue;
                            }
                            ParamKind::Rest => {
                                is_first_variadic_arg = false;
                            }
                            ParamKind::Positional => {}
                        }

                        let pos = arg_node.range().start;
                        let lsp_pos =
                            typst_to_lsp::offset_to_position(pos, self.encoding, self.source);

                        let label = InlayHintLabel::String(if info.kind == ParamKind::Rest {
                            format!("..{}:", info.param.name)
                        } else {
                            format!("{}:", info.param.name)
                        });

                        self.hints.push(InlayHint {
                            position: lsp_pos,
                            label,
                            kind: Some(InlayHintKind::PARAMETER),
                            text_edits: None,
                            tooltip: None,
                            padding_left: None,
                            padding_right: Some(true),
                            data: None,
                        });
                    }

                    // todo: union signatures
                }
                SyntaxKind::Set => {
                    trace!("set rule found: {:?}", node);
                }
                _ => {}
            }

            None
        }
    }

    let mut worker = InlayHintWorker {
        world,
        source,
        range,
        encoding,
        hints: vec![],
    };

    let root = LinkedNode::new(source.root());
    worker.analyze(root);

    Ok(worker.hints)
}

fn is_one_line(src: &Source, arg_node: &LinkedNode<'_>) -> bool {
    is_one_line_(src, arg_node).unwrap_or(true)
}

fn is_one_line_(src: &Source, arg_node: &LinkedNode<'_>) -> Option<bool> {
    let lb = arg_node.children().next()?;
    let rb = arg_node.children().next_back()?;
    let ll = src.byte_to_line(lb.offset())?;
    let rl = src.byte_to_line(rb.offset())?;
    Some(ll == rl)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn smart() {
        snapshot_testing("inlay_hints", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let request = InlayHintRequest {
                path: path.clone(),
                range: typst_to_lsp::range(
                    0..source.text().len(),
                    &source,
                    PositionEncoding::Utf16,
                ),
            };

            let result = request.request(ctx);
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
