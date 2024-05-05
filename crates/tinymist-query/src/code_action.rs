use lsp_types::TextEdit;
use once_cell::sync::OnceCell;

use crate::{prelude::*, SemanticRequest};

/// The [`textDocument/codeLens`] request is sent from the client to the server
/// to compute code lenses for a given text document.
///
/// [`textDocument/codeLens`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_codeLens
#[derive(Debug, Clone)]
pub struct CodeActionRequest {
    /// The path of the document to request for.
    pub path: PathBuf,
    /// The range of the document to get code actions for.
    pub range: LspRange,
}

impl SemanticRequest for CodeActionRequest {
    type Response = Vec<CodeActionOrCommand>;

    fn request(self, ctx: &mut AnalysisContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let range = ctx.to_typst_range(self.range, &source)?;
        let cursor = (range.start + 1).min(source.text().len());
        // todo: don't ignore the range end

        let root = LinkedNode::new(source.root());
        let mut worker = CodeActionWorker::new(ctx, source.clone());
        worker.work(root, cursor);

        let res = worker.actions;
        (!res.is_empty()).then_some(res)
    }
}

struct CodeActionWorker<'a, 'w> {
    ctx: &'a mut AnalysisContext<'w>,
    actions: Vec<CodeActionOrCommand>,
    local_url: OnceCell<Option<Url>>,
    current: Source,
}

impl<'a, 'w> CodeActionWorker<'a, 'w> {
    fn new(ctx: &'a mut AnalysisContext<'w>, current: Source) -> Self {
        Self {
            ctx,
            actions: Vec::new(),
            local_url: OnceCell::new(),
            current,
        }
    }

    fn local_url(&self) -> Option<&Url> {
        self.local_url
            .get_or_init(|| {
                let id = self.current.id();
                let path = self.ctx.path_for_id(id).ok()?;
                path_to_url(path.as_path()).ok()
            })
            .as_ref()
    }

    fn local_edit(&self, edit: TextEdit) -> Option<WorkspaceEdit> {
        Some(WorkspaceEdit {
            changes: Some(HashMap::from_iter([(
                self.local_url()?.clone(),
                vec![edit],
            )])),
            ..Default::default()
        })
    }

    fn heading_actions(&mut self, leaf: &LinkedNode) -> Option<()> {
        let h = leaf.cast::<ast::Heading>()?;
        let depth = h.depth().get();

        // Only the marker is replaced, for minimal text change
        let marker = leaf
            .children()
            .find(|e| e.kind() == SyntaxKind::HeadingMarker)?;
        let marker_range = marker.range();

        if depth > 1 {
            // decrease depth of heading
            let action = CodeActionOrCommand::CodeAction(CodeAction {
                title: "Decrease depth of heading".to_string(),
                kind: Some(CodeActionKind::REFACTOR_REWRITE),
                edit: Some(self.local_edit(TextEdit {
                    range: self.ctx.to_lsp_range(marker_range.clone(), &self.current),
                    new_text: "=".repeat(depth - 1),
                })?),
                ..CodeAction::default()
            });

            self.actions.push(action);
        }

        // increase depth of heading
        let action = CodeActionOrCommand::CodeAction(CodeAction {
            title: "Increase depth of heading".to_string(),
            kind: Some(CodeActionKind::REFACTOR_REWRITE),
            edit: Some(self.local_edit(TextEdit {
                range: self.ctx.to_lsp_range(marker_range, &self.current),
                new_text: "=".repeat(depth + 1),
            })?),
            ..CodeAction::default()
        });

        self.actions.push(action);

        Some(())
    }

    fn work(&mut self, root: LinkedNode, cursor: usize) -> Option<()> {
        let mut node = root.leaf_at(cursor)?;
        let mut heading_resolved = false;
        loop {
            match node.kind() {
                // Only the deepest heading is considered
                SyntaxKind::Heading if !heading_resolved => {
                    heading_resolved = true;
                    self.heading_actions(&node);
                }
                _ => {}
            }

            node = node.parent()?.clone();
        }
    }
}
