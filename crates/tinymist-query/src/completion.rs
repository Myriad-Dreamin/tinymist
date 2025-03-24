use crate::analysis::{CompletionCursor, CompletionWorker};
use crate::prelude::*;

pub(crate) mod proto;
pub use proto::*;
pub(crate) mod snippet;
pub use snippet::*;

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
    type Response = CompletionList;

    fn request(self, ctx: &mut LocalContext, graph: LspComputeGraph) -> Option<Self::Response> {
        // These trigger characters are for completion on positional arguments,
        // which follows the configuration item
        // `tinymist.completion.triggerOnSnippetPlaceholders`.
        if matches!(self.trigger_character, Some('(' | ',' | ':'))
            && !ctx.analysis.completion_feat.trigger_on_snippet_placeholders
        {
            return None;
        }

        let document = graph.snap.success_doc.as_ref();
        let source = ctx.source_by_path(&self.path).ok()?;
        let cursor = ctx.to_typst_pos_offset(&source, self.position, 0)?;

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
        //
        // Second try: According to VSCode:
        // - <https://github.com/microsoft/vscode/issues/130953>
        // - <https://github.com/microsoft/vscode/commit/0984071fe0d8a3c157a1ba810c244752d69e5689>
        // Checks the previous text to filter out letter explicit completions.
        //
        // Second try is failed.
        let explicit = false;
        let mut cursor = CompletionCursor::new(ctx.shared_(), &source, cursor)?;

        let mut worker = CompletionWorker::new(ctx, document, explicit, self.trigger_character)?;
        worker.work(&mut cursor)?;

        // todo: define it well, we were needing it because we wanted to do interactive
        // path completion, but now we've scanned all the paths at the same time.
        // is_incomplete = ic;
        let _ = worker.incomplete;

        // To response completions in fine-grained manner, we need to mark result as
        // incomplete. This follows what rust-analyzer does.
        // https://github.com/rust-lang/rust-analyzer/blob/f5a9250147f6569d8d89334dc9cca79c0322729f/crates/rust-analyzer/src/handlers/request.rs#L940C55-L940C75
        Some(CompletionList {
            is_incomplete: false,
            items: worker.completions,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use insta::with_settings;

    use super::*;
    use crate::{completion::proto::CompletionItem, syntax::find_module_level_docs, tests::*};

    struct TestConfig {
        pkg_mode: bool,
    }

    fn run(config: TestConfig) -> impl Fn(&mut LocalContext, PathBuf) {
        fn test(ctx: &mut LocalContext, id: TypstFileId) {
            let source = ctx.source_by_id(id).unwrap();
            let rng = find_test_range_(&source);
            let text = source.text()[rng.clone()].to_string();

            let docs = find_module_level_docs(&source).unwrap_or_default();
            let properties = get_test_properties(&docs);

            let trigger_character = properties
                .get("trigger_character")
                .map(|v| v.chars().next().unwrap());
            let explicit = match properties.get("explicit").copied().map(str::trim) {
                Some("true") => true,
                Some("false") | None => false,
                Some(v) => panic!("invalid value for 'explicit' property: {v}"),
            };

            let mut includes = HashSet::new();
            let mut excludes = HashSet::new();

            let graph = compile_doc_for_test(ctx, &properties);

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
                        command: item.command,
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
                    path: ctx.path_for_id(id).unwrap().as_path().to_owned(),
                    position: ctx.to_lsp_pos(s, &source),
                    explicit,
                    trigger_character,
                };
                let result = request
                    .request(ctx, graph.clone())
                    .map(|list| CompletionList {
                        is_incomplete: list.is_incomplete,
                        items: get_items(list.items),
                    });
                results.push(result);
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
