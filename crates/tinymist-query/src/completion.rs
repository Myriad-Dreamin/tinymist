use lsp_types::{
    Command, CompletionItemLabelDetails, CompletionList, CompletionTextEdit, InsertTextFormat,
    TextEdit,
};
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use typst_shim::syntax::LinkedNodeExt;

use crate::{
    analysis::{InsTy, Ty},
    prelude::*,
    syntax::{is_ident_like, SyntaxClass},
    upstream::{autocomplete, CompletionContext},
    StatefulRequest,
};

pub(crate) type LspCompletion = lsp_types::CompletionItem;
pub(crate) type LspCompletionKind = lsp_types::CompletionItemKind;
pub(crate) type TypstCompletionKind = crate::upstream::CompletionKind;

pub(crate) mod snippet;

/// The [`textDocument/completion`] request is sent from the client to the
/// server to compute completion items at a given cursor position.
///
/// If computing full completion items is expensive, servers can additionally
/// provide a handler for the completion item resolve request
/// (`completionItem/resolve`). This request is sent when a completion item is
/// selected in the user interface.
///
/// [`textDocument/completion`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_completion
///
/// # Compatibility
///
/// Since 3.16.0, the client can signal that it can resolve more properties
/// lazily. This is done using the `completion_item.resolve_support` client
/// capability which lists all properties that can be filled in during a
/// `completionItem/resolve` request.
///
/// All other properties (usually `sort_text`, `filter_text`, `insert_text`, and
/// `text_edit`) must be provided in the `textDocument/completion` response and
/// must not be changed during resolve.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    /// The path of the document to compute completions.
    pub path: PathBuf,
    /// The position in the document at which to compute completions.
    pub position: LspPosition,
    /// Whether the completion is triggered explicitly.
    pub explicit: bool,
    /// The character that triggered the completion, if any.
    pub trigger_character: Option<char>,
}

impl StatefulRequest for CompletionRequest {
    type Response = CompletionResponse;

    fn request(
        self,
        ctx: &mut LocalContext,
        doc: Option<VersionedDocument>,
    ) -> Option<Self::Response> {
        // These trigger characters are for completion on positional arguments,
        // which follows the configuration item
        // `tinymist.completion.triggerOnSnippetPlaceholders`.
        if matches!(self.trigger_character, Some('(' | ',' | ':'))
            && !ctx.analysis.completion_feat.trigger_on_snippet_placeholders
        {
            return None;
        }

        let doc = doc.as_ref().map(|doc| doc.document.as_ref());
        let source = ctx.source_by_path(&self.path).ok()?;
        let (cursor, syntax) = ctx.classify_pos_(&source, self.position, 0)?;

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

        // Skip if is the let binding item *directly*
        if let Some(SyntaxClass::VarAccess(var)) = &syntax {
            let node = var.node();
            match node.parent_kind() {
                // complete the init part of the let binding
                Some(SyntaxKind::LetBinding) => {
                    let parent = node.parent()?;
                    let parent_init = parent.cast::<ast::LetBinding>()?.init()?;
                    let parent_init = parent.find(parent_init.span())?;
                    parent_init.find(node.span())?;
                }
                Some(SyntaxKind::Closure) => {
                    let parent = node.parent()?;
                    let parent_body = parent.cast::<ast::Closure>()?.body();
                    let parent_body = parent.find(parent_body.span())?;
                    parent_body.find(node.span())?;
                }
                _ => {}
            }
        }

        // Skip if an error node starts with number (e.g. `1pt`)
        if matches!(
            syntax,
            Some(SyntaxClass::Callee(..) | SyntaxClass::VarAccess(..) | SyntaxClass::Normal(..))
        ) {
            let node = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
            if node.erroneous() {
                let mut chars = node.text().chars();

                match chars.next() {
                    Some(ch) if ch.is_numeric() => return None,
                    Some('.') => {
                        if matches!(chars.next(), Some(ch) if ch.is_numeric()) {
                            return None;
                        }
                    }
                    _ => {}
                }
            }
        }

        let mut completion_items_rest = None;
        let is_incomplete = false;

        let mut cc_ctx =
            CompletionContext::new(ctx, doc, &source, cursor, explicit, self.trigger_character)?;

        // Exclude it self from auto completion
        // e.g. `#let x = (1.);`
        let self_ty = cc_ctx.leaf.cast::<ast::Expr>().and_then(|leaf| {
            let v = cc_ctx.ctx.mini_eval(leaf)?;
            Some(Ty::Value(InsTy::new(v)))
        });

        if let Some(self_ty) = self_ty {
            cc_ctx.seen_types.insert(self_ty);
        };

        let (offset, ic, mut completions, completions_items2) = autocomplete(cc_ctx)?;
        if !completions_items2.is_empty() {
            completion_items_rest = Some(completions_items2);
        }
        // todo: define it well, we were needing it because we wanted to do interactive
        // path completion, but now we've scanned all the paths at the same time.
        // is_incomplete = ic;
        let _ = ic;

        // Filter and determine range to replace
        let mut from_ident = None;
        let is_callee = matches!(syntax, Some(SyntaxClass::Callee(..)));
        if matches!(
            syntax,
            Some(SyntaxClass::Callee(..) | SyntaxClass::VarAccess(..))
        ) {
            let node = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
            if is_ident_like(&node) && node.offset() == offset {
                from_ident = Some(node);
            }
        }
        let replace_range = if let Some(from_ident) = from_ident {
            let mut rng = from_ident.range();
            let ident_prefix = source.text()[rng.start..cursor].to_string();

            completions.retain(|item| {
                let mut prefix_matcher = item.label.chars();
                'ident_matching: for ch in ident_prefix.chars() {
                    for item in prefix_matcher.by_ref() {
                        if item == ch {
                            continue 'ident_matching;
                        }
                    }

                    return false;
                }

                true
            });

            // if modifying some arguments, we need to truncate and add a comma
            if !is_callee && cursor != rng.end && is_arg_like_context(&from_ident) {
                // extend comma
                for item in completions.iter_mut() {
                    let apply = match &mut item.apply {
                        Some(w) => w,
                        None => {
                            item.apply = Some(item.label.clone());
                            item.apply.as_mut().unwrap()
                        }
                    };
                    if apply.trim_end().ends_with(',') {
                        continue;
                    }
                    apply.push_str(", ");
                }

                // Truncate
                rng.end = cursor;
            }

            ctx.to_lsp_range(rng, &source)
        } else {
            ctx.to_lsp_range(offset..cursor, &source)
        };

        let completions = completions.iter().map(|typst_completion| {
            let typst_snippet = typst_completion
                .apply
                .as_ref()
                .unwrap_or(&typst_completion.label);
            let lsp_snippet = to_lsp_snippet(typst_snippet);
            let text_edit = CompletionTextEdit::Edit(TextEdit::new(replace_range, lsp_snippet));

            LspCompletion {
                label: typst_completion.label.to_string(),
                kind: Some(completion_kind(typst_completion.kind.clone())),
                detail: typst_completion.detail.as_ref().map(String::from),
                sort_text: typst_completion.sort_text.as_ref().map(String::from),
                filter_text: typst_completion.filter_text.as_ref().map(String::from),
                label_details: typst_completion.label_detail.as_ref().map(|desc| {
                    CompletionItemLabelDetails {
                        detail: None,
                        description: Some(desc.to_string()),
                    }
                }),
                text_edit: Some(text_edit),
                additional_text_edits: typst_completion.additional_text_edits.clone(),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                commit_characters: typst_completion
                    .commit_char
                    .as_ref()
                    .map(|v| vec![v.to_string()]),
                command: typst_completion.command.as_ref().map(|cmd| Command {
                    command: cmd.to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }
        });
        let mut items = completions.collect_vec();

        if let Some(items_rest) = completion_items_rest.as_mut() {
            items.append(items_rest);
        }

        // To response completions in fine-grained manner, we need to mark result as
        // incomplete. This follows what rust-analyzer does.
        // https://github.com/rust-lang/rust-analyzer/blob/f5a9250147f6569d8d89334dc9cca79c0322729f/crates/rust-analyzer/src/handlers/request.rs#L940C55-L940C75
        Some(CompletionResponse::List(CompletionList {
            is_incomplete,
            items,
        }))
    }
}

pub(crate) fn completion_kind(typst_completion_kind: TypstCompletionKind) -> LspCompletionKind {
    match typst_completion_kind {
        TypstCompletionKind::Syntax => LspCompletionKind::SNIPPET,
        TypstCompletionKind::Func => LspCompletionKind::FUNCTION,
        TypstCompletionKind::Param => LspCompletionKind::VARIABLE,
        TypstCompletionKind::Field => LspCompletionKind::FIELD,
        TypstCompletionKind::Variable => LspCompletionKind::VARIABLE,
        TypstCompletionKind::Constant => LspCompletionKind::CONSTANT,
        TypstCompletionKind::Reference => LspCompletionKind::REFERENCE,
        TypstCompletionKind::Symbol(_) => LspCompletionKind::FIELD,
        TypstCompletionKind::Type => LspCompletionKind::CLASS,
        TypstCompletionKind::Module => LspCompletionKind::MODULE,
        TypstCompletionKind::File => LspCompletionKind::FILE,
        TypstCompletionKind::Folder => LspCompletionKind::FOLDER,
    }
}

static TYPST_SNIPPET_PLACEHOLDER_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\$\{(.*?)\}").unwrap());

/// Adds numbering to placeholders in snippets
fn to_lsp_snippet(typst_snippet: &EcoString) -> String {
    let mut counter = 1;
    let result =
        TYPST_SNIPPET_PLACEHOLDER_RE.replace_all(typst_snippet.as_str(), |cap: &Captures| {
            let substitution = format!("${{{}:{}}}", counter, &cap[1]);
            counter += 1;
            substitution
        });

    result.to_string()
}

fn is_arg_like_context(mut matching: &LinkedNode) -> bool {
    while let Some(parent) = matching.parent() {
        use SyntaxKind::*;

        // todo: contextual
        match parent.kind() {
            ContentBlock | Equation | CodeBlock | Markup | Math | Code => return false,
            Args | Params | Destructuring | Array | Dict => return true,
            _ => {}
        }

        matching = parent;
    }
    false
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use insta::with_settings;
    use lsp_types::CompletionItem;

    use super::*;
    use crate::{syntax::find_module_level_docs, tests::*};

    struct TestConfig {
        pkg_mode: bool,
    }

    fn run(config: TestConfig) -> impl Fn(&mut LocalContext, PathBuf) {
        fn test(ctx: &mut LocalContext, id: TypstFileId) {
            let source = ctx.source_by_id(id).unwrap();
            let rng = find_test_range(&source);
            let text = source.text()[rng.clone()].to_string();

            let docs = find_module_level_docs(&source).unwrap_or_default();
            let properties = get_test_properties(&docs);

            let trigger_character = properties
                .get("trigger_character")
                .map(|v| v.chars().next().unwrap());

            let mut includes = HashSet::new();
            let mut excludes = HashSet::new();

            let doc = compile_doc_for_test(ctx, &properties);

            for kk in properties.get("contains").iter().flat_map(|v| v.split(',')) {
                // split first char
                let (kind, item) = kk.split_at(1);
                if kind == "+" {
                    includes.insert(item.trim());
                } else if kind == "-" {
                    excludes.insert(item.trim());
                } else {
                    includes.insert(kk.trim());
                }
            }
            let get_items = |items: Vec<CompletionItem>| {
                let mut res: Vec<_> = items
                    .into_iter()
                    .filter(|item| {
                        if !excludes.is_empty() && excludes.contains(item.label.as_str()) {
                            panic!("{item:?} was excluded in {excludes:?}");
                        }
                        if includes.is_empty() {
                            return true;
                        }
                        includes.contains(item.label.as_str())
                    })
                    .map(|item| CompletionItem {
                        label: item.label,
                        label_details: item.label_details,
                        sort_text: item.sort_text,
                        kind: item.kind,
                        text_edit: item.text_edit,
                        ..Default::default()
                    })
                    .collect();

                res.sort_by(|a, b| {
                    a.sort_text
                        .as_ref()
                        .cmp(&b.sort_text.as_ref())
                        .then_with(|| a.label.cmp(&b.label))
                });
                res
            };

            let mut results = vec![];
            for s in rng.clone() {
                let request = CompletionRequest {
                    path: ctx.path_for_id(id).unwrap(),
                    position: ctx.to_lsp_pos(s, &source),
                    explicit: false,
                    trigger_character,
                };
                results.push(request.request(ctx, doc.clone()).map(|resp| match resp {
                    CompletionResponse::List(list) => CompletionResponse::List(CompletionList {
                        is_incomplete: list.is_incomplete,
                        items: get_items(list.items),
                    }),
                    CompletionResponse::Array(items) => CompletionResponse::Array(get_items(items)),
                }));
            }
            with_settings!({
                description => format!("Completion on {text} ({rng:?})"),
            }, {
                assert_snapshot!(JsonRepr::new_pure(results));
            })
        }

        move |ctx, path| {
            if config.pkg_mode {
                let files = ctx
                    .source_files()
                    .iter()
                    .filter(|id| !id.vpath().as_rootless_path().ends_with("lib.typ"));
                for id in files.copied().collect::<Vec<_>>() {
                    test(ctx, id);
                }
            } else {
                test(ctx, ctx.file_id_by_path(&path).unwrap());
            }
        }
    }

    #[test]
    fn test_base() {
        snapshot_testing("completion", &run(TestConfig { pkg_mode: false }));
    }

    #[test]
    fn test_pkgs() {
        snapshot_testing("pkgs", &run(TestConfig { pkg_mode: true }));
    }
}
