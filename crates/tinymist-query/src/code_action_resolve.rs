use std::path::PathBuf;

use ecow::EcoVec;
use lsp_types::Diagnostic;
use tinymist_lint::KnownLintIssues;
use tinymist_project::{CompiledArtifact, LspComputeGraph};
use typst::syntax::LinkedNode;

use crate::analysis::CodeActionResolveWorker;
use crate::code_action::proto::*;
use crate::{DiagWorker, LocalContext, StatefulRequest};

/// A `codeAction/resolve` request.
#[derive(Debug, Clone)]
pub struct CodeActionResolveRequest {
    /// The path of the document to request for.
    ///
    /// Note that the `codeAction/resolve` request does not specify the document directly, so we
    /// pass the document URL in the data field by convention when sending deferred code actions, and
    /// parse it back ourselves when handling the request.
    ///
    /// This is the same approach that the [Ruff language server takes][ruff].
    ///
    /// [ruff]: https://github.com/astral-sh/ruff/blob/v0.4.10/crates/ruff_server/src/server/api/requests/code_action_resolve.rs#L23-L26.
    pub path: PathBuf,

    /// The code action to resolve edits for.
    pub action: lsp_types::CodeAction,
}

impl StatefulRequest for CodeActionResolveRequest {
    type Response = Box<CodeAction>;

    fn request(self, ctx: &mut LocalContext, graph: LspComputeGraph) -> Option<Self::Response> {
        log::trace!("resolving code action: {self:?}");

        let kind = self.action.kind?;
        let source = ctx.source_by_path(&self.path).ok()?;
        let root = LinkedNode::new(source.root());

        let diagnostics: EcoVec<Diagnostic> = {
            // Rerun the compiler to make sure we have the latest diagnostics.
            // TODO: is this the correct API to use?
            let is_html = ctx.world.library.features.is_enabled(typst::Feature::Html);
            let art = CompiledArtifact::from_graph(graph, is_html);

            let compiler_diags = art.diagnostics();

            // TODO: It seems a bit wasteful that we convert `SourceDiagnostic`s to LSP diagnostics
            // in `DiagWorker`, and then convert back to Typst positions in
            // `CodeActionResolveWorker`.
            let worker = DiagWorker::new(ctx);
            let known_issues = KnownLintIssues::from_compiler_diagnostics(compiler_diags.clone());
            worker
                .check(&source, &known_issues)
                .convert_all(compiler_diags)
                .into_values()
                .flatten()
                .collect()
        };

        let edit = CodeActionResolveWorker::try_new(ctx, source.clone(), diagnostics)?
            .resolve_edit(&root, &kind)?;

        Some(Box::new(CodeAction {
            title: self.action.title,
            kind: Some(kind),
            edit: Some(edit),
            ..CodeAction::default()
        }))
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::CodeActionKind;

    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("code_action_resolve", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let docs = find_module_level_docs(&source).unwrap_or_default();
            let properties = get_test_properties(&docs);
            let resolve_kind = CodeActionKind::from(
                properties
                    .get("resolve")
                    .expect("a `resolve` property specifying the code action kind")
                    .to_string(),
            );

            let graph = WorldComputeGraph::from_world(ctx.world.clone());

            let action = lsp_types::CodeAction {
                kind: Some(resolve_kind),
                // Leave the other fields blank; we'll only snapshot the edit.
                ..lsp_types::CodeAction::default()
            };
            let request = CodeActionResolveRequest {
                path: path.clone(),
                action,
            };
            let result = request.request(ctx, graph);
            assert_snapshot!(JsonRepr::new_redacted(
                result.map(|action| action.edit),
                &REDACT_LOC
            ));
        })
    }
}
