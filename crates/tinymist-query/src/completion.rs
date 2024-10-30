use lsp_types::{
    Command, CompletionItemLabelDetails, CompletionList, CompletionTextEdit, InsertTextFormat,
    TextEdit,
};
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use typst_shim::syntax::LinkedNodeExt;

use crate::{
    analysis::{BuiltinTy, InsTy, Ty},
    prelude::*,
    syntax::{is_ident_like, DerefTarget},
    upstream::{autocomplete, complete_path, CompletionContext},
    StatefulRequest,
};

pub(crate) type LspCompletion = lsp_types::CompletionItem;
pub(crate) type LspCompletionKind = lsp_types::CompletionItemKind;
pub(crate) type TypstCompletionKind = crate::upstream::CompletionKind;

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
    /// Whether to trigger suggest completion, a.k.a. auto-completion.
    pub trigger_suggest: bool,
    /// Whether to trigger named parameter completion.
    pub trigger_named_completion: bool,
    /// Whether to trigger parameter hint, a.k.a. signature help.
    pub trigger_parameter_hints: bool,
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
        let (cursor, deref_target) = ctx.deref_syntax_at_(&source, self.position, 0)?;

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
        if let Some(DerefTarget::VarAccess(node)) = &deref_target {
            match node.parent_kind() {
                // complete the init part of the let binding
                Some(SyntaxKind::LetBinding) => {
                    let parent = node.parent()?;
                    let parent_init = parent.cast::<ast::LetBinding>()?.init()?;
                    let parent_init = parent.find(parent_init.span())?;
                    parent_init.find(node.span())?;
                }
                Some(SyntaxKind::Closure) => return None,
                _ => {}
            }
        }

        // Do some completion specific to the deref target
        let mut ident_like = None;
        let mut completion_result = None;
        let is_callee = matches!(deref_target, Some(DerefTarget::Callee(..)));
        match deref_target {
            Some(DerefTarget::Callee(..) | DerefTarget::VarAccess(..)) => {
                let node = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
                if is_ident_like(&node) {
                    ident_like = Some(node);
                }
            }
            Some(DerefTarget::ImportPath(v) | DerefTarget::IncludePath(v)) => {
                if !v.text().starts_with(r#""@"#) {
                    completion_result = complete_path(
                        ctx,
                        Some(v),
                        &source,
                        cursor,
                        &crate::analysis::PathPreference::Source,
                    );
                }
            }
            Some(DerefTarget::Normal(SyntaxKind::Str, cano_expr)) => {
                let parent = cano_expr.parent()?;
                if matches!(parent.kind(), SyntaxKind::Named | SyntaxKind::Args) {
                    let ty_chk = ctx.type_check(&source);

                    let ty = ty_chk.type_of_span(cano_expr.span());
                    log::debug!("check string ty: {ty:?}");
                    if let Some(Ty::Builtin(BuiltinTy::Path(path_filter))) = ty {
                        completion_result =
                            complete_path(ctx, Some(cano_expr), &source, cursor, &path_filter);
                    }
                }
            }
            Some(DerefTarget::Label(..) | DerefTarget::Ref(..) | DerefTarget::Normal(..)) => {}
            None => {}
        }

        let mut completion_items_rest = None;
        let is_incomplete = false;

        let mut items = completion_result.or_else(|| {
            let mut cc_ctx = CompletionContext::new(
                ctx,
                doc,
                &source,
                cursor,
                explicit,
                self.trigger_character,
                self.trigger_suggest,
                self.trigger_parameter_hints,
                self.trigger_named_completion,
            )?;

            // Exclude it self from auto completion
            // e.g. `#let x = (1.);`
            let self_ty = cc_ctx.leaf.cast::<ast::Expr>().and_then(|exp| {
                let v = cc_ctx.ctx.mini_eval(exp)?;
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

            let replace_range;
            if ident_like.as_ref().is_some_and(|i| i.offset() == offset) {
                let ident_like = ident_like.unwrap();
                let mut rng = ident_like.range();
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

                // if modifying some arguments, we need to truncate and add a comma
                if !is_callee && cursor != rng.end && is_arg_like_context(&ident_like) {
                    // extend comma
                    for c in completions.iter_mut() {
                        let apply = match &mut c.apply {
                            Some(w) => w,
                            None => {
                                c.apply = Some(c.label.clone());
                                c.apply.as_mut().unwrap()
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

                replace_range = ctx.to_lsp_range(rng, &source);
            } else {
                replace_range = ctx.to_lsp_range(offset..cursor, &source);
            }

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
                    label_details: typst_completion.label_detail.as_ref().map(|e| {
                        CompletionItemLabelDetails {
                            detail: None,
                            description: Some(e.to_string()),
                        }
                    }),
                    text_edit: Some(text_edit),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    command: typst_completion.command.as_ref().map(|c| Command {
                        command: c.to_string(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            });

            Some(completions.collect_vec())
        })?;

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

    fn run(c: TestConfig) -> impl Fn(&mut AnalysisContext, PathBuf) {
        fn test(ctx: &mut AnalysisContext, id: TypstFileId) {
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
                    trigger_suggest: true,
                    trigger_parameter_hints: true,
                    trigger_named_completion: true,
                };
                results.push(request.request(ctx, doc.clone()).map(|resp| match resp {
                    CompletionResponse::List(l) => CompletionResponse::List(CompletionList {
                        is_incomplete: l.is_incomplete,
                        items: get_items(l.items),
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
            if c.pkg_mode {
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
