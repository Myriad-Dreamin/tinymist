use lsp_types::CodeActionContext;

use crate::{SemanticRequest, analysis::CodeActionWorker, prelude::*};

pub(crate) mod proto;
pub use proto::*;

/// The [`textDocument/codeAction`] request is sent from the client to the
/// server to compute commands for a given text document and range. These
/// commands are typically code fixes to either fix problems or to
/// beautify/refactor code.
///
/// The result of a [`textDocument/codeAction`] request is an array of `Command`
/// literals which are typically presented in the user interface.
///
/// [`textDocument/codeAction`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_codeAction
///
/// To ensure that a server is useful in many clients, the commands specified in
/// a code actions should be handled by the server and not by the client (see
/// [`workspace/executeCommand`] and
/// `ServerCapabilities::execute_command_provider`). If the client supports
/// providing edits with a code action, then the mode should be used.
///
/// When the command is selected the server should be contacted again (via the
/// [`workspace/executeCommand`] request) to execute the command.
///
/// [`workspace/executeCommand`]: https://microsoft.github.io/language-server-protocol/specification#workspace_executeCommand
///
/// # Compatibility
///
/// ## Since version 3.16.0
///
/// A client can offer a server to delay the computation of code action
/// properties during a `textDocument/codeAction` request. This is useful for
/// cases where it is expensive to compute the value of a property (for example,
/// the `edit` property).
///
/// Clients signal this through the `code_action.resolve_support` client
/// capability which lists all properties a client can resolve lazily. The
/// server capability `code_action_provider.resolve_provider` signals that a
/// server will offer a `codeAction/resolve` route.
///
/// To help servers uniquely identify a code action in the resolve request, a
/// code action literal may optionally carry a `data` property. This is also
/// guarded by an additional client capability `code_action.data_support`. In
/// general, a client should offer data support if it offers resolve support.
///
/// It should also be noted that servers shouldn’t alter existing attributes of
/// a code action in a `codeAction/resolve` request.
///
/// ## Since version 3.8.0
///
/// Support for [`CodeAction`] literals to enable the following scenarios:
///
/// * The ability to directly return a workspace edit from the code action
///   request. This avoids having another server roundtrip to execute an actual
///   code action. However server providers should be aware that if the code
///   action is expensive to compute or the edits are huge it might still be
///   beneficial if the result is simply a command and the actual edit is only
///   computed when needed.
///
/// * The ability to group code actions using a kind. Clients are allowed to
///   ignore that information. However it allows them to better group code
///   action, for example, into corresponding menus (e.g. all refactor code
///   actions into a refactor menu).
#[derive(Debug, Clone)]
pub struct CodeActionRequest {
    /// The path of the document to request for.
    pub path: PathBuf,
    /// The range of the document to get code actions for.
    pub range: LspRange,
    /// The context of the code action request.
    pub context: CodeActionContext,
}

impl SemanticRequest for CodeActionRequest {
    type Response = Vec<CodeAction>;

    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response> {
        log::info!("requested code action: {self:?}");

        let source = ctx.source_by_path(&self.path).ok()?;
        let range = ctx.to_typst_range(self.range, &source)?;

        let root = LinkedNode::new(source.root());
        let mut worker = CodeActionWorker::new(ctx, source.clone());
        worker.autofix(&root, &range, &self.context);
        worker.scoped(&root, &range);

        (!worker.actions.is_empty()).then_some(worker.actions)
    }
}

#[cfg(test)]
mod tests {
    use typst::{diag::Warned, layout::PagedDocument};

    use super::*;
    use crate::{DiagWorker, tests::*};

    #[test]
    fn test() {
        snapshot_testing("code_action", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let request_range = find_test_range(&source);
            let code_action_ctx = compute_code_action_context(ctx, &source, &request_range);
            let request = CodeActionRequest {
                path: path.clone(),
                range: request_range,
                context: code_action_ctx,
            };

            let result = request.request(ctx);

            with_settings!({
                description => format!("Code Action on {}", make_range_annotation(&source)),
            }, {
                assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
            })
        });
    }

    fn compute_code_action_context(
        ctx: &mut LocalContext,
        source: &Source,
        request_range: &LspRange,
    ) -> CodeActionContext {
        let Warned { output, warnings } = typst::compile::<PagedDocument>(&ctx.world);
        let errors = output.err().unwrap_or_default();
        let compiler_diagnostics = warnings.iter().chain(errors.iter());

        // Run the linter for additional diagnostics as well.
        let diagnostics = DiagWorker::new(ctx)
            .check(source)
            .convert_all(compiler_diagnostics)
            .into_values()
            .flatten();

        CodeActionContext {
            // The filtering here matches the LSP specification and VS Code behavior;
            // see https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#codeActionContext:
            //   `diagnostics`: An array of diagnostics known on the client side overlapping the range
            //   provided to the textDocument/codeAction request [...]
            diagnostics: diagnostics
                .filter(|diag| ranges_overlap(&diag.range, request_range))
                .collect(),
            only: None,
            trigger_kind: None,
        }
    }

    fn ranges_overlap(r1: &LspRange, r2: &LspRange) -> bool {
        !(r1.end <= r2.start || r2.end <= r1.start)
    }
}
