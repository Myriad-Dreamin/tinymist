use crate::{prelude::*, SemanticRequest};

/// The [`textDocument/documentHighlight`] request
///
/// [`textDocument/documentHighlight`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_documentHighlight
#[derive(Debug, Clone)]
pub struct DocumentHighlightRequest {
    /// The path of the document to request highlight for.
    pub path: PathBuf,
    /// The position of the document to request highlight for.
    pub position: LspPosition,
}

impl SemanticRequest for DocumentHighlightRequest {
    type Response = Vec<DocumentHighlight>;

    fn request(self, ctx: &mut AnalysisContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let cursor = ctx.to_typst_pos(self.position, &source)?;

        let root = LinkedNode::new(source.root());
        let mut node = &root.leaf_at(cursor)?;

        loop {
            match node.kind() {
                SyntaxKind::For
                | SyntaxKind::While
                | SyntaxKind::Break
                | SyntaxKind::Continue
                | SyntaxKind::LoopBreak
                | SyntaxKind::LoopContinue => {
                    return DocumentHighlightWorker::new(ctx, &source).highlight_loop_of(node)
                }
                SyntaxKind::Arrow
                | SyntaxKind::Params
                | SyntaxKind::Return
                | SyntaxKind::FuncReturn => return highlight_func_returns(ctx, node),
                _ => {}
            }
            node = node.parent()?;
        }
    }
}

struct DocumentHighlightWorker<'a, 'w> {
    ctx: &'a mut AnalysisContext<'w>,
    current: &'a Source,
    highlights: Vec<DocumentHighlight>,
    worklist: Vec<LinkedNode<'a>>,
}

impl<'a, 'w> DocumentHighlightWorker<'a, 'w> {
    fn new(ctx: &'a mut AnalysisContext<'w>, current: &'a Source) -> Self {
        Self {
            ctx,
            current,
            highlights: Vec::new(),
            worklist: Vec::new(),
        }
    }

    fn finish(self) -> Option<Vec<DocumentHighlight>> {
        (!self.highlights.is_empty()).then_some(self.highlights)
    }

    fn annotate(&mut self, node: &LinkedNode) {
        let mut rng = node.range();

        // if previous node is hash
        if rng.start > 0 && self.current.text().as_bytes()[rng.start - 1] == b'#' {
            rng.start -= 1;
        }

        self.highlights.push(DocumentHighlight {
            range: self.ctx.to_lsp_range(rng, self.current),
            kind: None,
        });
    }

    fn check<F>(&mut self, check: F)
    where
        F: Fn(&mut Self, LinkedNode<'a>),
    {
        while let Some(node) = self.worklist.pop() {
            check(self, node);
        }
    }

    fn check_children(&mut self, node: &LinkedNode<'a>) {
        if node.get().children().len() == 0 {
            return;
        }

        for child in node.children() {
            self.worklist.push(child.clone());
        }
    }

    fn check_loop(&mut self, node: LinkedNode<'a>) {
        match node.kind() {
            SyntaxKind::ForLoop
            | SyntaxKind::WhileLoop
            | SyntaxKind::Closure
            | SyntaxKind::Contextual => {
                return;
            }
            SyntaxKind::LoopBreak | SyntaxKind::LoopContinue => {
                self.annotate(&node);
                return;
            }
            _ => {}
        }

        self.check_children(&node);
    }

    fn highlight_loop_of(mut self, node: &'a LinkedNode<'a>) -> Option<Vec<DocumentHighlight>> {
        let _ = self.ctx;

        // find the nearest loop node
        let loop_node = ancestors(node)
            .find(|node| matches!(node.kind(), SyntaxKind::ForLoop | SyntaxKind::WhileLoop))?;

        // find the first key word of the loop node
        let keyword = loop_node.children().find(|node| node.kind().is_keyword());
        if let Some(keyword) = keyword {
            self.annotate(&keyword);
        }

        self.check_children(loop_node);
        self.check(Self::check_loop);

        log::debug!("highlights: {:?}", self.highlights);
        self.finish()
    }
}

fn highlight_func_returns(
    ctx: &mut AnalysisContext,
    node: &LinkedNode,
) -> Option<Vec<DocumentHighlight>> {
    let _ = ctx;
    let _ = node;
    None
}

fn ancestors<'a>(node: &'a LinkedNode<'a>) -> impl Iterator<Item = &'a LinkedNode<'a>> {
    std::iter::successors(Some(node), |node| node.parent())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("document_highlight", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let request = DocumentHighlightRequest {
                path: path.clone(),
                position: find_test_position_after(&source),
            };

            let result = request.request(ctx);
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
