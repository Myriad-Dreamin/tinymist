//! Resolves edits for expensive code actions.

use crate::analysis::code_action::{
    AutofixKind, SOURCE_TYPST_SPACE_UNKNOWN_MATH_VARS, match_autofix_kind,
    suggest_math_unknown_variable_spaces_edit,
};
use crate::prelude::*;

/// Resolves edits for a code action.
pub struct CodeActionResolveWorker<'a> {
    /// The local analysis context to work with.
    ctx: &'a mut LocalContext,
    /// The source document.
    source: Source,
    /// The combined compiler and linter diagnostics for the source file.
    diagnostics: EcoVec<Diagnostic>,
    /// The local URL to [`Self::source`].
    local_url: Url,
}

impl<'a> CodeActionResolveWorker<'a> {
    /// Attempts to create a new code action resolve worker.
    pub fn try_new(
        ctx: &'a mut LocalContext,
        source: Source,
        diagnostics: EcoVec<Diagnostic>,
    ) -> Option<Self> {
        let local_url = ctx.uri_for_id(source.id()).ok()?;
        Some(Self {
            ctx,
            source,
            diagnostics,
            local_url,
        })
    }

    #[must_use]
    fn local_edits(&self, edits: Vec<EcoSnippetTextEdit>) -> EcoWorkspaceEdit {
        EcoWorkspaceEdit {
            changes: Some(HashMap::from_iter([(self.local_url.clone(), edits)])),
            ..Default::default()
        }
    }
}

impl CodeActionResolveWorker<'_> {
    /// Resolves edits for a source-level code action of the given kind.
    pub fn resolve_edit(
        self,
        root: &LinkedNode,
        kind: &CodeActionKind,
    ) -> Option<EcoWorkspaceEdit> {
        if kind == &SOURCE_TYPST_SPACE_UNKNOWN_MATH_VARS {
            Some(self.resolve_space_all_unknown_math_vars(root))
        } else {
            log::warn!("unexpected code action kind in `codeAction/resolve` request: {kind:?}");
            None
        }
    }

    fn resolve_space_all_unknown_math_vars(&self, root: &LinkedNode) -> EcoWorkspaceEdit {
        let fix = |diag: &Diagnostic| -> Option<EcoSnippetTextEdit> {
            let range = self.ctx.to_typst_range(diag.range, &self.source)?;
            let cursor = (range.start + 1).min(self.source.text().len());
            let node = root.leaf_at_compat(cursor)?;
            suggest_math_unknown_variable_spaces_edit(self.ctx, &self.source, &node)
        };

        self.local_edits(
            self.diagnostics
                .iter()
                .filter(|diag| {
                    match_autofix_kind(&diag.message) == Some(AutofixKind::UnknownVariable)
                })
                .flat_map(fix)
                .collect(),
        )
    }
}
