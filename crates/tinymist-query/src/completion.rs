use ecow::eco_format;
use lsp_types::{CompletionItem, CompletionList, CompletionTextEdit, InsertTextFormat, TextEdit};
use reflexo::path::{unix_slash, PathClean};

use crate::{
    prelude::*,
    syntax::{get_deref_target, DerefTarget},
    typst_to_lsp::completion_kind,
    upstream::{autocomplete_, Completion, CompletionContext, CompletionKind},
    LspCompletion, StatefulRequest,
};

use self::typst_to_lsp::completion;

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
        let cursor = {
            let mut cursor = ctx.to_typst_pos(self.position, &source)?;
            let text = source.text();

            // while is not char boundary, move cursor to right
            while cursor < text.len() && !text.is_char_boundary(cursor) {
                cursor += 1;
            }

            cursor
        };

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

        let root = LinkedNode::new(source.root());
        let node = root.leaf_at(cursor);
        let deref_target = node.and_then(|node| get_deref_target(node, cursor));

        let mut match_ident = None;
        let mut completion_result = None;
        match deref_target {
            Some(DerefTarget::Callee(v) | DerefTarget::VarAccess(v)) => {
                if v.is::<ast::Ident>() {
                    match_ident = Some(v);
                }
            }
            Some(DerefTarget::ImportPath(v)) => {
                if !v.text().starts_with(r#""@"#) {
                    completion_result = complete_path(ctx, v, &source, cursor);
                }
            }
            None => {}
        }

        let items = completion_result.or_else(|| {
            let cc_ctx = CompletionContext::new(ctx.world(), doc, &source, cursor, explicit)?;
            let (offset, mut completions) = autocomplete_(cc_ctx)?;

            let replace_range;
            if match_ident.as_ref().is_some_and(|i| i.offset() == offset) {
                let match_ident = match_ident.unwrap();
                let rng = match_ident.range();
                replace_range = ctx.to_lsp_range(match_ident.range(), &source);

                let ident_prefix = source.text()[rng.start..cursor].to_string();
                completions.retain(|c| {
                    // c.label
                    let mut prefix_matcher = c.label.chars();
                    'ident_matching: for ch in ident_prefix.chars() {
                        for c in prefix_matcher.by_ref() {
                            if c == ch {
                                continue 'ident_matching;
                            }
                        }

                        return false;
                    }

                    true
                });
            } else {
                let lsp_start_position = ctx.to_lsp_pos(offset, &source);
                replace_range = LspRange::new(lsp_start_position, self.position);
            }

            Some(
                completions
                    .iter()
                    .map(|typst_completion| completion(typst_completion, replace_range))
                    .collect_vec(),
            )
        })?;

        // To response completions in fine-grained manner, we need to mark result as
        // incomplete. This follows what rust-analyzer does.
        // https://github.com/rust-lang/rust-analyzer/blob/f5a9250147f6569d8d89334dc9cca79c0322729f/crates/rust-analyzer/src/handlers/request.rs#L940C55-L940C75
        Some(CompletionResponse::List(CompletionList {
            is_incomplete: true,
            items,
        }))
    }
}

fn complete_path(
    ctx: &AnalysisContext,
    v: LinkedNode,
    source: &Source,
    cursor: usize,
) -> Option<Vec<CompletionItem>> {
    let id = source.id();
    if id.package().is_some() {
        return None;
    }

    let vp = v.cast::<ast::Str>()?;
    // todo: path escape
    let real_content = vp.get();
    let text = v.text();
    let unquoted = &text[1..text.len() - 1];
    if unquoted != real_content {
        return None;
    }

    let text = source.text();
    let vr = v.range();
    let offset = vr.start + 1;
    if cursor < offset || vr.end <= cursor || vr.len() < 2 {
        return None;
    }
    let path = Path::new(&text[offset..cursor]);
    let is_abs = path.is_absolute();

    let src_path = id.vpath();
    let base = src_path.resolve(&ctx.analysis.root)?;
    let dst_path = src_path.join(path);
    let mut compl_path = dst_path.as_rootless_path();
    if !compl_path.is_dir() {
        compl_path = compl_path.parent().unwrap_or(Path::new(""));
    }
    log::info!("compl_path: {src_path:?} + {path:?} -> {compl_path:?}");

    if compl_path.is_absolute() {
        log::warn!("absolute path completion is not supported for security consideration {path:?}");
        return None;
    }

    let dirs = ctx.analysis.root.join(compl_path);
    log::info!("compl_dirs: {dirs:?}");
    // find directory or files in the path
    let mut folder_completions = vec![];
    let mut module_completions = vec![];
    // todo: test it correctly
    for entry in dirs.read_dir().ok()? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        log::trace!("compl_check_path: {path:?}");
        if !path.is_dir() && !path.extension().is_some_and(|ext| ext == "typ") {
            continue;
        }
        if path.is_dir()
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        {
            continue;
        }

        // diff with root
        let path = dirs.join(path);

        // Skip self smartly
        if path.clean() == base.clean() {
            continue;
        }

        let label = if is_abs {
            // diff with root
            let w = path.strip_prefix(&ctx.analysis.root).ok()?;
            eco_format!("/{}", unix_slash(w))
        } else {
            let base = base.parent()?;
            let w = pathdiff::diff_paths(&path, base)?;
            unix_slash(&w).into()
        };
        log::info!("compl_label: {label:?}");

        if path.is_dir() {
            folder_completions.push(Completion {
                label,
                kind: CompletionKind::Folder,
                apply: None,
                detail: None,
            });
        } else {
            module_completions.push(Completion {
                label,
                kind: CompletionKind::Module,
                apply: None,
                detail: None,
            });
        }
    }

    let rng = offset..vr.end - 1;
    let replace_range = ctx.to_lsp_range(rng, source);

    module_completions.sort_by(|a, b| a.label.cmp(&b.label));
    folder_completions.sort_by(|a, b| a.label.cmp(&b.label));

    let mut sorter = 0;
    let digits = (module_completions.len() + folder_completions.len())
        .to_string()
        .len();
    let completions = module_completions.into_iter().chain(folder_completions);
    Some(
        completions
            .map(|typst_completion| {
                let lsp_snippet = typst_completion
                    .apply
                    .as_ref()
                    .unwrap_or(&typst_completion.label);
                let text_edit =
                    CompletionTextEdit::Edit(TextEdit::new(replace_range, lsp_snippet.to_string()));

                let sort_text = format!("{sorter:0>digits$}");
                sorter += 1;

                let res = LspCompletion {
                    label: typst_completion.label.to_string(),
                    kind: Some(completion_kind(typst_completion.kind.clone())),
                    detail: typst_completion.detail.as_ref().map(String::from),
                    text_edit: Some(text_edit),
                    // don't sort me
                    sort_text: Some(sort_text),
                    filter_text: Some("".to_owned()),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    ..Default::default()
                };

                log::info!("compl_res: {res:?}");

                res
            })
            .collect_vec(),
    )
}

#[cfg(test)]
mod tests {
    use insta::with_settings;
    use lsp_types::{CompletionItem, CompletionList};

    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("completion", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();
            let rng = find_test_range(&source);
            let text = source.text()[rng.clone()].to_string();

            let mut results = vec![];
            for s in rng.clone() {
                let request = CompletionRequest {
                    path: path.clone(),
                    position: ctx.to_lsp_pos(s, &source),
                    explicit: false,
                };
                results.push(request.request(ctx, None).map(|resp| {
                    // CompletionResponse::Array(items)
                    match resp {
                        CompletionResponse::List(l) => CompletionResponse::List(CompletionList {
                            is_incomplete: l.is_incomplete,
                            items: l
                                .items
                                .into_iter()
                                .map(|item| CompletionItem {
                                    label: item.label,
                                    kind: item.kind,
                                    text_edit: item.text_edit,
                                    ..Default::default()
                                })
                                .collect(),
                        }),
                        CompletionResponse::Array(items) => CompletionResponse::Array(
                            items
                                .into_iter()
                                .map(|item| CompletionItem {
                                    label: item.label,
                                    kind: item.kind,
                                    text_edit: item.text_edit,
                                    ..Default::default()
                                })
                                .collect(),
                        ),
                    }
                }));
            }
            with_settings!({
                description => format!("Completion on {text} ({rng:?})"),
            }, {
                assert_snapshot!(JsonRepr::new_pure(results));
            })
        });
    }
}
