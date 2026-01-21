use ecow::eco_format;

use crate::analysis::CodeActionWorker;
use crate::prelude::*;
use crate::syntax::{InterpretMode, interpret_mode_at};

impl<'a> CodeActionWorker<'a> {
    pub(super) fn figure_actions(&mut self, node: &LinkedNode) -> Option<()> {
        match node.kind() {
            SyntaxKind::FuncCall => self.figure_actions_for_func_call(node),
            SyntaxKind::CodeBlock => self.figure_actions_for_code_block(node),
            SyntaxKind::ContentBlock => self.figure_actions_for_content_block(node),
            SyntaxKind::Raw => self.figure_actions_for_raw(node),
            _ => None,
        }
    }

    fn figure_actions_for_func_call(&mut self, node: &LinkedNode) -> Option<()> {
        let call = node.cast::<ast::FuncCall>()?;
        let callee = call.callee();

        // Check if this is an image, table, or raw function call
        let func_name = match callee {
            ast::Expr::Ident(ident) => ident.get().as_str(),
            _ => return None,
        };

        if !matches!(func_name, "image" | "table" | "raw") {
            return None;
        }

        // For function calls, don't add hash - if it's needed, it should already be present
        self.wrap_in_figure(node, func_name, false)
    }

    fn figure_actions_for_code_block(&mut self, node: &LinkedNode) -> Option<()> {
        let _block = node.cast::<ast::CodeBlock>()?;
        self.wrap_in_figure(node, "code block", false)
    }

    fn figure_actions_for_content_block(&mut self, node: &LinkedNode) -> Option<()> {
        let _block = node.cast::<ast::ContentBlock>()?;
        self.wrap_in_figure(node, "content block", false)
    }

    fn figure_actions_for_raw(&mut self, node: &LinkedNode) -> Option<()> {
        // For raw blocks (backticks), determine if we need hash based on the context
        let needs_hash = matches!(
            interpret_mode_at(Some(node.parent()?)),
            InterpretMode::Markup | InterpretMode::Math
        );
        self.wrap_in_figure(node, "raw block", needs_hash)
    }

    fn wrap_in_figure(&mut self, node: &LinkedNode, func_name: &str, add_hash: bool) -> Option<()> {
        let mode = if add_hash { "#" } else { "" };

        // Get the full range of the function call or code block
        let call_range = node.range();
        let call_text = self.source.text().get(call_range.clone())?;

        // Create action for caption before
        let caption_before = if self.ctx.analysis.extended_code_action {
            eco_format!(
                "{mode}figure(\n  caption: [${{1:Caption}}],\n  {}\n)$0",
                call_text
            )
        } else {
            eco_format!("{mode}figure(\n  caption: [Caption],\n  {}\n)", call_text)
        };

        let edit_before = self.local_edit(EcoSnippetTextEdit::new(
            self.ctx.to_lsp_range(call_range.clone(), &self.source),
            caption_before,
        ))?;

        let action_before = CodeAction {
            title: tinymist_l10n::t!(
                "tinymist-query.code-action.wrapFigureCaptionBefore",
                "Wrap {func_name} in figure with caption before",
                func_name = func_name.into()
            )
            .to_string(),
            kind: Some(CodeActionKind::REFACTOR_REWRITE),
            edit: Some(edit_before),
            ..CodeAction::default()
        };
        self.actions.push(action_before);

        // Create action for caption after
        let caption_after = if self.ctx.analysis.extended_code_action {
            eco_format!(
                "{mode}figure(\n  {},\n  caption: [${{1:Caption}}],\n)$0",
                call_text
            )
        } else {
            eco_format!("{mode}figure(\n  {},\n  caption: [Caption],\n)", call_text)
        };

        let edit_after = self.local_edit(EcoSnippetTextEdit::new(
            self.ctx.to_lsp_range(call_range, &self.source),
            caption_after,
        ))?;

        let action_after = CodeAction {
            title: tinymist_l10n::t!(
                "tinymist-query.code-action.wrapFigureCaptionAfter",
                "Wrap {func_name} in figure with caption after",
                func_name = func_name.into()
            )
            .to_string(),
            kind: Some(CodeActionKind::REFACTOR_REWRITE),
            edit: Some(edit_after),
            ..CodeAction::default()
        };
        self.actions.push(action_after);

        Some(())
    }
}
