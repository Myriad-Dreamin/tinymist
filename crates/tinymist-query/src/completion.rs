use crate::{prelude::*, StatefulRequest};

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub path: PathBuf,
    pub position: LspPosition,
    pub explicit: bool,
}

impl StatefulRequest for CompletionRequest {
    type Response = CompletionResponse;

    fn request(
        self,
        ctx: &mut AnalysisContext,
        doc: Option<VersionedDocument>,
    ) -> Option<Self::Response> {
        let doc = doc.as_ref().map(|doc| doc.document.as_ref());
        let source = ctx.source_by_path(&self.path).ok()?;
        let cursor = ctx.to_typst_pos(self.position, &source)?;

        // Please see <https://github.com/nvarner/typst-lsp/commit/2d66f26fb96ceb8e485f492e5b81e9db25c3e8ec>
        //
        // FIXME: correctly identify a completion which is triggered
        // by explicit action, such as by pressing control and space
        // or something similar.
        //
        // See <https://github.com/microsoft/language-server-protocol/issues/1101>
        // > As of LSP 3.16, CompletionTriggerKind takes the value Invoked for
        // > both manually invoked (for ex: ctrl + space in VSCode) completions
        // > and always on (what the spec refers to as 24/7 completions).
        //
        // Hence, we cannot distinguish between the two cases. Conservatively, we
        // assume that the completion is not explicit.
        let explicit = false;

        let (offset, completions) =
            typst_ide::autocomplete(ctx.world, doc, &source, cursor, explicit)?;

        let lsp_start_position = ctx.to_lsp_pos(offset, &source);
        let replace_range = LspRange::new(lsp_start_position, self.position);
        Some(typst_to_lsp::completions(&completions, replace_range).into())
    }
}
