//! Resolves edits for expensive code actions.

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
        None
    }
}
