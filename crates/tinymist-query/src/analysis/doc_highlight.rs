//! Analyze related expressions to highlight in a source file.

use crate::{prelude::*, syntax::node_ancestors};

/// Analyzes the document and provides related expression information to
/// highlight.
pub struct DocumentHighlightWorker<'a> {
    /// The local analysis context to work with.
    ctx: &'a mut LocalContext,
    /// The source document to analyze.
    source: &'a Source,
    /// The related expressions to provide.
    pub annotated: Vec<DocumentHighlight>,
    /// The worklist to check the nodes.
    worklist: Vec<LinkedNode<'a>>,
}

impl<'a> DocumentHighlightWorker<'a> {
    /// Creates a new worker
    pub fn new(ctx: &'a mut LocalContext, source: &'a Source) -> Self {
        Self {
            ctx,
            source,
            annotated: Vec::new(),
            worklist: Vec::new(),
        }
    }

    /// Starts to work
    pub fn work(&mut self, mut node: &'a LinkedNode<'a>) -> Option<()> {
        loop {
            match node.kind() {
                SyntaxKind::For
                | SyntaxKind::While
                | SyntaxKind::Break
                | SyntaxKind::Continue
                | SyntaxKind::LoopBreak
                | SyntaxKind::LoopContinue => return self.work_loop(node),
                SyntaxKind::Arrow
                | SyntaxKind::Params
                | SyntaxKind::Return
                | SyntaxKind::FuncReturn
                | SyntaxKind::Contextual => return self.work_func(node),
                _ => {}
            }
            node = node.parent()?;
        }
    }

    fn work_loop(&mut self, node: &'a LinkedNode<'a>) -> Option<()> {
        let _ = self.ctx;

        // find the nearest loop node
        let loop_node = 'find_loop: {
            for anc in node_ancestors(node) {
                if matches!(anc.kind(), SyntaxKind::Contextual | SyntaxKind::Closure) {
                    return None;
                }
                if matches!(anc.kind(), SyntaxKind::ForLoop | SyntaxKind::WhileLoop) {
                    break 'find_loop anc;
                }
            }
            return None;
        };

        // find the first key word of the loop node
        let keyword = loop_node.children().find(|node| node.kind().is_keyword());
        if let Some(keyword) = keyword {
            self.annotate(&keyword);
        }

        self.check_children(loop_node);
        self.check(Self::check_loop);

        crate::log_debug_ct!("highlights: {:?}", self.annotated);
        Some(())
    }

    fn work_func(&mut self, _node: &'a LinkedNode<'a>) -> Option<()> {
        None
    }

    /// Annotate the node for highlight
    fn annotate(&mut self, node: &LinkedNode) {
        let mut rng = node.range();

        // if previous node is hash
        if rng.start > 0 && self.source.text().as_bytes()[rng.start - 1] == b'#' {
            rng.start -= 1;
        }

        self.annotated.push(DocumentHighlight {
            range: self.ctx.to_lsp_range(rng, self.source),
            kind: None,
        });
    }

    /// Consumes the worklist and checks the nodes
    fn check<F>(&mut self, check: F)
    where
        F: Fn(&mut Self, LinkedNode<'a>),
    {
        while let Some(node) = self.worklist.pop() {
            check(self, node);
        }
    }

    /// Pushes the children of the node to check
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
}
