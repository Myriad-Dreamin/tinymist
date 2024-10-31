use lsp_types::TextEdit;
use once_cell::sync::{Lazy, OnceCell};
use regex::Regex;
use typst_shim::syntax::LinkedNodeExt;

use crate::{prelude::*, SemanticRequest};

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
}

impl SemanticRequest for CodeActionRequest {
    type Response = Vec<CodeActionOrCommand>;

    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response> {
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

struct CodeActionWorker<'a> {
    ctx: &'a mut LocalContext,
    actions: Vec<CodeActionOrCommand>,
    local_url: OnceCell<Option<Url>>,
    current: Source,
}

impl<'a> CodeActionWorker<'a> {
    fn new(ctx: &'a mut LocalContext, current: Source) -> Self {
        Self {
            ctx,
            actions: Vec::new(),
            local_url: OnceCell::new(),
            current,
        }
    }

    fn local_url(&self) -> Option<&Url> {
        self.local_url
            .get_or_init(|| self.ctx.uri_for_id(self.current.id()).ok())
            .as_ref()
    }

    fn local_edits(&self, edits: Vec<TextEdit>) -> Option<WorkspaceEdit> {
        Some(WorkspaceEdit {
            changes: Some(HashMap::from_iter([(self.local_url()?.clone(), edits)])),
            ..Default::default()
        })
    }

    fn local_edit(&self, edit: TextEdit) -> Option<WorkspaceEdit> {
        self.local_edits(vec![edit])
    }

    fn heading_actions(&mut self, node: &LinkedNode) -> Option<()> {
        let h = node.cast::<ast::Heading>()?;
        let depth = h.depth().get();

        // Only the marker is replaced, for minimal text change
        let marker = node
            .children()
            .find(|e| e.kind() == SyntaxKind::HeadingMarker)?;
        let marker_range = marker.range();

        if depth > 1 {
            // Decrease depth of heading
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

        // Increase depth of heading
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

    fn equation_actions(&mut self, node: &LinkedNode) -> Option<()> {
        let equation = node.cast::<ast::Equation>()?;
        let body = equation.body();
        let is_block = equation.block();

        let body = node.find(body.span())?;
        let body_range = body.range();
        let node_end = node.range().end;

        let mut chs = node.children();
        let chs = chs.by_ref();
        let first_dollar = chs.take(1).find(|e| e.kind() == SyntaxKind::Dollar)?;
        let last_dollar = chs.rev().take(1).find(|e| e.kind() == SyntaxKind::Dollar)?;

        // Erroneous equation is skipped.
        // For example, some unclosed equation.
        if first_dollar.offset() == last_dollar.offset() {
            return None;
        }

        let front_range = self
            .ctx
            .to_lsp_range(first_dollar.range().end..body_range.start, &self.current);
        let back_range = self
            .ctx
            .to_lsp_range(body_range.end..last_dollar.range().start, &self.current);

        // Retrive punctuation to move
        let mark_after_equation = self
            .current
            .text()
            .get(node_end..)
            .and_then(|text| {
                let mut ch = text.chars();
                let nx = ch.next()?;
                Some((nx, ch.next()))
            })
            .filter(|(ch, ch_next)| {
                static IS_PUNCTUATION: Lazy<Regex> =
                    Lazy::new(|| Regex::new(r"\p{Punctuation}").unwrap());
                (ch.is_ascii_punctuation()
                    && ch_next.map_or(true, |ch_next| !ch_next.is_ascii_punctuation()))
                    || (!ch.is_ascii_punctuation() && IS_PUNCTUATION.is_match(&ch.to_string()))
            });
        let punc_modify = if let Some((nx, _)) = mark_after_equation {
            let ch_range = self
                .ctx
                .to_lsp_range(node_end..node_end + nx.len_utf8(), &self.current);
            let remove_edit = TextEdit {
                range: ch_range,
                new_text: "".to_owned(),
            };
            Some((nx, remove_edit))
        } else {
            None
        };

        let rewrite_action = |title: &str, new_text: &str| {
            let mut edits = vec![
                TextEdit {
                    range: front_range,
                    new_text: new_text.to_owned(),
                },
                TextEdit {
                    range: back_range,
                    new_text: if !new_text.is_empty() {
                        if let Some((ch, _)) = &punc_modify {
                            ch.to_string() + new_text
                        } else {
                            new_text.to_owned()
                        }
                    } else {
                        "".to_owned()
                    },
                },
            ];

            if !new_text.is_empty() {
                if let Some((_, edit)) = &punc_modify {
                    edits.push(edit.clone());
                }
            }

            Some(CodeActionOrCommand::CodeAction(CodeAction {
                title: title.to_owned(),
                kind: Some(CodeActionKind::REFACTOR_REWRITE),
                edit: Some(self.local_edits(edits)?),
                ..CodeAction::default()
            }))
        };

        // Prepare actions
        let a1 = if is_block {
            rewrite_action("Convert to inline equation", "")?
        } else {
            rewrite_action("Convert to block equation", " ")?
        };
        let a2 = rewrite_action("Convert to multiple-line block equation", "\n");

        self.actions.push(a1);
        if let Some(a2) = a2 {
            self.actions.push(a2);
        }

        Some(())
    }

    fn work(&mut self, root: LinkedNode, cursor: usize) -> Option<()> {
        let node = root.leaf_at_compat(cursor)?;
        let mut node = &node;

        let mut heading_resolved = false;
        let mut equation_resolved = false;

        loop {
            match node.kind() {
                // Only the deepest heading is considered
                SyntaxKind::Heading if !heading_resolved => {
                    heading_resolved = true;
                    self.heading_actions(node);
                }
                // Only the deepest equation is considered
                SyntaxKind::Equation if !equation_resolved => {
                    equation_resolved = true;
                    self.equation_actions(node);
                }
                _ => {}
            }

            node = node.parent()?;
        }
    }
}
