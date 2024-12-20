use core::fmt::{self, Write};

use typst::foundations::repr::separated_list;
use typst_shim::syntax::LinkedNodeExt;

use crate::analysis::get_link_exprs_in;
use crate::jump_from_cursor;
use crate::prelude::*;
use crate::upstream::{expr_tooltip, route_of_value, truncated_repr, Tooltip};

/// The [`textDocument/hover`] request asks the server for hover information at
/// a given text document position.
///
/// [`textDocument/hover`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_hover
///
/// Such hover information typically includes type signature information and
/// inline documentation for the symbol at the given text document position.
#[derive(Debug, Clone)]
pub struct HoverRequest {
    /// The path of the document to get hover information for.
    pub path: PathBuf,
    /// The position of the symbol to get hover information for.
    pub position: LspPosition,
}

impl StatefulRequest for HoverRequest {
    type Response = Hover;

    fn request(
        self,
        ctx: &mut LocalContext,
        doc: Option<VersionedDocument>,
    ) -> Option<Self::Response> {
        let doc_ref = doc.as_ref().map(|doc| doc.document.as_ref());

        let source = ctx.source_by_path(&self.path).ok()?;
        let offset = ctx.to_typst_pos(self.position, &source)?;
        // the typst's cursor is 1-based, so we need to add 1 to the offset
        let cursor = offset + 1;

        let node = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
        let range = ctx.to_lsp_range(node.range(), &source);

        let contents = def_tooltip(ctx, &source, doc.as_ref(), cursor)
            .or_else(|| star_tooltip(ctx, &node))
            .or_else(|| link_tooltip(ctx, &node, cursor))
            .or_else(|| Some(to_lsp_tooltip(&ctx.tooltip(doc_ref, &source, cursor)?)))?;

        // Neovim shows ugly hover if the hover content is in array, so we join them
        // manually with divider bars.
        let mut contents = match contents {
            HoverContents::Array(contents) => contents
                .into_iter()
                .map(|content| match content {
                    MarkedString::LanguageString(content) => {
                        format!("```{}\n{}\n```", content.language, content.value)
                    }
                    MarkedString::String(content) => content,
                })
                .join("\n\n---\n"),
            HoverContents::Scalar(MarkedString::String(contents)) => contents,
            HoverContents::Scalar(MarkedString::LanguageString(contents)) => {
                format!("```{}\n{}\n```", contents.language, contents.value)
            }
            lsp_types::HoverContents::Markup(content) => {
                match content.kind {
                    MarkupKind::Markdown => content.value,
                    // todo: escape
                    MarkupKind::PlainText => content.value,
                }
            }
        };

        if let Some(provider) = ctx.analysis.periscope.clone() {
            if let Some(doc) = doc.clone() {
                let position = jump_from_cursor(&doc.document, &source, cursor);
                let position = position.or_else(|| {
                    for idx in 1..100 {
                        let next_cursor = cursor + idx;
                        if next_cursor < source.text().len() {
                            let position = jump_from_cursor(&doc.document, &source, next_cursor);
                            if position.is_some() {
                                return position;
                            }
                        }
                        let prev_cursor = cursor.checked_sub(idx);
                        if let Some(prev_cursor) = prev_cursor {
                            let position = jump_from_cursor(&doc.document, &source, prev_cursor);
                            if position.is_some() {
                                return position;
                            }
                        }
                    }

                    None
                });

                log::info!("telescope position: {:?}", position);
                let content = position.and_then(|pos| provider.periscope_at(ctx, doc, pos));
                if let Some(preview_content) = content {
                    contents = format!("{preview_content}\n---\n{contents}");
                }
            }
        }

        Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String(contents)),
            range: Some(range),
        })
    }
}

fn def_tooltip(
    ctx: &mut LocalContext,
    source: &Source,
    document: Option<&VersionedDocument>,
    cursor: usize,
) -> Option<HoverContents> {
    let leaf = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
    let syntax = classify_node(leaf.clone(), cursor)?;
    let def = ctx.def_of_syntax(source, document, syntax.clone())?;

    let mut results = vec![];
    let mut actions = vec![];

    use Decl::*;
    match def.decl.as_ref() {
        Label(..) => {
            results.push(MarkedString::String(format!("Label: {}\n", def.name())));
            // todo: type repr
            if let Some(val) = def.term.as_ref().and_then(|v| v.value()) {
                let repr = truncated_repr(&val);
                results.push(MarkedString::String(format!("{repr}")));
            }
            Some(HoverContents::Array(results))
        }
        BibEntry(..) => {
            results.push(MarkedString::String(format!(
                "Bibliography: @{}",
                def.name()
            )));

            Some(HoverContents::Array(results))
        }
        _ => {
            let sym_docs = ctx.def_docs(&def);

            if matches!(def.decl.kind(), DefKind::Variable | DefKind::Constant) {
                // todo: check sensible length, value highlighting
                if let Some(values) = expr_tooltip(ctx.world(), syntax.node()) {
                    match values {
                        Tooltip::Text(values) => {
                            results.push(MarkedString::String(values.into()));
                        }
                        Tooltip::Code(values) => {
                            results.push(MarkedString::LanguageString(LanguageString {
                                language: "typc".to_owned(),
                                value: values.into(),
                            }));
                        }
                    }
                }
            }

            // todo: hover with `with_stack`

            if matches!(
                def.decl.kind(),
                DefKind::Function | DefKind::Variable | DefKind::Constant
            ) && !def.name().is_empty()
            {
                results.push(MarkedString::LanguageString(LanguageString {
                    language: "typc".to_owned(),
                    value: {
                        let mut type_doc = String::new();
                        type_doc.push_str("let ");
                        type_doc.push_str(def.name());

                        match &sym_docs {
                            Some(DefDocs::Variable(docs)) => {
                                push_result_ty(def.name(), docs.return_ty.as_ref(), &mut type_doc);
                            }
                            Some(DefDocs::Function(docs)) => {
                                let _ = docs.print(&mut type_doc);
                                push_result_ty(def.name(), docs.ret_ty.as_ref(), &mut type_doc);
                            }
                            _ => {}
                        }
                        type_doc.push(';');
                        type_doc
                    },
                }));
            }

            if let Some(doc) = sym_docs {
                results.push(MarkedString::String(doc.hover_docs().into()));
            }

            if let Some(link) = ExternalDocLink::get(&def) {
                actions.push(link);
            }

            render_actions(&mut results, actions);
            Some(HoverContents::Array(results))
        }
    }
}

fn star_tooltip(ctx: &mut LocalContext, mut node: &LinkedNode) -> Option<HoverContents> {
    if !matches!(node.kind(), SyntaxKind::Star) {
        return None;
    }

    while !matches!(node.kind(), SyntaxKind::ModuleImport) {
        node = node.parent()?;
    }

    let import_node = node.cast::<ast::ModuleImport>()?;
    let scope_val = ctx.module_by_syntax(import_node.source().to_untyped())?;

    let scope_items = scope_val.scope()?.iter();
    let mut names = scope_items.map(|item| item.0.as_str()).collect::<Vec<_>>();
    names.sort();

    let content = format!("This star imports {}", separated_list(&names, "and"));
    Some(HoverContents::Scalar(MarkedString::String(content)))
}

fn link_tooltip(
    ctx: &mut LocalContext,
    mut node: &LinkedNode,
    cursor: usize,
) -> Option<HoverContents> {
    while !matches!(node.kind(), SyntaxKind::FuncCall) {
        node = node.parent()?;
    }

    let links = get_link_exprs_in(node)?;
    let links = links
        .objects
        .iter()
        .filter(|link| link.range.contains(&cursor))
        .collect::<Vec<_>>();
    if links.is_empty() {
        return None;
    }

    let mut results = vec![];
    let mut actions = vec![];
    for obj in links {
        let Some(target) = obj.target.resolve(ctx) else {
            continue;
        };
        // open file in tab or system application
        actions.push(CommandLink {
            title: Some("Open in Tab".to_string()),
            command_or_links: vec![CommandOrLink::Command {
                id: "tinymist.openInternal".to_string(),
                args: vec![JsonValue::String(target.to_string())],
            }],
        });
        actions.push(CommandLink {
            title: Some("Open Externally".to_string()),
            command_or_links: vec![CommandOrLink::Command {
                id: "tinymist.openExternal".to_string(),
                args: vec![JsonValue::String(target.to_string())],
            }],
        });
        if let Some(kind) = PathPreference::from_ext(target.path()) {
            let preview = format!("A `{kind:?}` file.");
            results.push(MarkedString::String(preview));
        }
    }
    render_actions(&mut results, actions);
    if results.is_empty() {
        return None;
    }

    Some(HoverContents::Array(results))
}

fn push_result_ty(
    name: &str,
    ty_repr: Option<&(EcoString, EcoString, EcoString)>,
    type_doc: &mut String,
) {
    let Some((short, _, _)) = ty_repr else {
        return;
    };
    if short == name {
        return;
    }

    let _ = write!(type_doc, " = {short}");
}

fn render_actions(results: &mut Vec<MarkedString>, actions: Vec<CommandLink>) {
    if actions.is_empty() {
        return;
    }

    results.push(MarkedString::String(actions.into_iter().join(" | ")));
}

struct ExternalDocLink;

impl ExternalDocLink {
    fn get(def: &Definition) -> Option<CommandLink> {
        let value = def.value();

        if matches!(value, Some(Value::Func(..))) {
            if let Some(builtin) = Self::builtin_func_tooltip("https://typst.app/docs/", def) {
                return Some(builtin);
            }
        };

        value.and_then(|value| Self::builtin_value_tooltip("https://typst.app/docs/", &value))
    }

    fn builtin_func_tooltip(base: &str, def: &Definition) -> Option<CommandLink> {
        let Some(Value::Func(func)) = def.value() else {
            return None;
        };

        use typst::foundations::func::Repr;
        let mut func = &func;
        loop {
            match func.inner() {
                Repr::Element(..) | Repr::Native(..) => {
                    return Self::builtin_value_tooltip(base, &Value::Func(func.clone()));
                }
                Repr::With(w) => {
                    func = &w.0;
                }
                Repr::Closure(..) => {
                    return None;
                }
            }
        }
    }

    fn builtin_value_tooltip(base: &str, value: &Value) -> Option<CommandLink> {
        let base = base.trim_end_matches('/');
        let route = route_of_value(value)?;
        let link = format!("{base}/{route}");
        Some(CommandLink {
            title: Some("Open docs".to_owned()),
            command_or_links: vec![CommandOrLink::Link(link)],
        })
    }
}

struct CommandLink {
    title: Option<String>,
    command_or_links: Vec<CommandOrLink>,
}

impl fmt::Display for CommandLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // https://github.com/rust-lang/rust-analyzer/blob/1a5bb27c018c947dab01ab70ffe1d267b0481a17/editors/code/src/client.ts#L59
        let title = self.title.as_deref().unwrap_or("");
        let command_or_links = self.command_or_links.iter().join(" ");
        write!(f, "[{title}]({command_or_links})")
    }
}

enum CommandOrLink {
    Link(String),
    Command { id: String, args: Vec<JsonValue> },
}

impl fmt::Display for CommandOrLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Link(link) => f.write_str(link),
            Self::Command { id, args } => {
                // <https://code.visualstudio.com/api/extension-guides/command#command-uris>
                if args.is_empty() {
                    return write!(f, "command:{id}");
                }

                let args = serde_json::to_string(&args).unwrap();
                let args = percent_encoding::utf8_percent_encode(
                    &args,
                    percent_encoding::NON_ALPHANUMERIC,
                );
                write!(f, "command:{id}?{args}")
            }
        }
    }
}

fn to_lsp_tooltip(typst_tooltip: &Tooltip) -> HoverContents {
    let lsp_marked_string = match typst_tooltip {
        Tooltip::Text(text) => MarkedString::String(text.to_string()),
        Tooltip::Code(code) => MarkedString::LanguageString(LanguageString {
            language: "typc".to_owned(),
            value: code.to_string(),
        }),
    };
    HoverContents::Scalar(lsp_marked_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("hover", &|world, path| {
            let source = world.source_by_path(&path).unwrap();

            let request = HoverRequest {
                path: path.clone(),
                position: find_test_position(&source),
            };

            let result = request.request(world, None);
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
