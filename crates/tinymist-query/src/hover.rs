use core::fmt::{self, Write};

use tinymist_std::typst::TypstDocument;
use typst::foundations::repr::separated_list;
use typst_shim::syntax::LinkedNodeExt;

use crate::analysis::get_link_exprs_in;
use crate::bib::{render_citation_string, RenderedBibCitation};
use crate::jump_from_cursor;
use crate::prelude::*;
use crate::upstream::{route_of_value, truncated_repr, Tooltip};

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

    fn request(self, ctx: &mut LocalContext, graph: LspComputeGraph) -> Option<Self::Response> {
        let doc = graph.snap.success_doc.clone();
        let source = ctx.source_by_path(&self.path).ok()?;
        let offset = ctx.to_typst_pos(self.position, &source)?;
        // the typst's cursor is 1-based, so we need to add 1 to the offset
        let cursor = offset + 1;

        let node = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
        let range = ctx.to_lsp_range(node.range(), &source);

        let mut worker = HoverWorker {
            ctx,
            source,
            doc,
            cursor,
            def: Default::default(),
            value: Default::default(),
            preview: Default::default(),
            docs: Default::default(),
            actions: Default::default(),
        };

        worker.work();

        let mut contents = vec![];

        contents.append(&mut worker.def);
        contents.append(&mut worker.value);
        contents.append(&mut worker.preview);
        contents.append(&mut worker.docs);
        if !worker.actions.is_empty() {
            let content = worker.actions.into_iter().join(" | ");
            contents.push(content);
        }

        if contents.is_empty() {
            return None;
        }

        Some(Hover {
            // Neovim shows ugly hover if the hover content is in array, so we join them
            // manually with divider bars.
            contents: HoverContents::Scalar(MarkedString::String(contents.join("\n\n---\n\n"))),
            range: Some(range),
        })
    }
}

struct HoverWorker<'a> {
    ctx: &'a mut LocalContext,
    source: Source,
    doc: Option<TypstDocument>,
    cursor: usize,
    def: Vec<String>,
    value: Vec<String>,
    preview: Vec<String>,
    docs: Vec<String>,
    actions: Vec<CommandLink>,
}

impl HoverWorker<'_> {
    fn work(&mut self) {
        self.static_analysis();
        self.preview();
        self.dynamic_analysis();
    }

    /// Static analysis results
    fn static_analysis(&mut self) -> Option<()> {
        let source = self.source.clone();
        let leaf = LinkedNode::new(source.root()).leaf_at_compat(self.cursor)?;

        self.definition()
            .or_else(|| self.star(&leaf))
            .or_else(|| self.link(&leaf))
    }

    /// Dynamic analysis results
    fn dynamic_analysis(&mut self) -> Option<()> {
        let typst_tooltip = self.ctx.tooltip(&self.source, self.cursor)?;
        self.value.push(match typst_tooltip {
            Tooltip::Text(text) => text.to_string(),
            Tooltip::Code(code) => format!("### Sampled Values\n```typc\n{code}\n```"),
        });
        Some(())
    }

    /// Definition analysis results
    fn definition(&mut self) -> Option<()> {
        let leaf = LinkedNode::new(self.source.root()).leaf_at_compat(self.cursor)?;
        let syntax = classify_syntax(leaf.clone(), self.cursor)?;
        let def = self
            .ctx
            .def_of_syntax(&self.source, self.doc.as_ref(), syntax.clone())?;

        use Decl::*;
        match def.decl.as_ref() {
            Label(..) => {
                if let Some(val) = def.term.as_ref().and_then(|v| v.value()) {
                    self.def.push(format!("Ref: `{}`\n", def.name()));
                    self.def
                        .push(format!("```typc\n{}\n```", truncated_repr(&val)));
                } else {
                    self.def.push(format!("Label: `{}`\n", def.name()));
                }
            }
            BibEntry(..) => {
                if let Some(details) = try_get_bib_details(&self.doc, self.ctx, def.name()) {
                    self.def.push(format!(
                        "Bibliography: `{}` {}",
                        def.name(),
                        details.citation
                    ));
                    self.def.push(details.bib_item);
                } else {
                    // fallback: no additional information
                    self.def.push(format!("Bibliography: `{}`", def.name()));
                }
            }
            _ => {
                let sym_docs = self.ctx.def_docs(&def);

                // todo: hover with `with_stack`

                if matches!(
                    def.decl.kind(),
                    DefKind::Function | DefKind::Variable | DefKind::Constant
                ) && !def.name().is_empty()
                {
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

                    self.def.push(format!("```typc\n{type_doc};\n```"));
                }

                if let Some(doc) = sym_docs {
                    let hover_docs = doc.hover_docs();

                    if !hover_docs.trim().is_empty() {
                        self.docs.push(hover_docs.into());
                    }
                }

                if let Some(link) = ExternalDocLink::get(&def) {
                    self.actions.push(link);
                }
            }
        }

        Some(())
    }

    fn star(&mut self, mut node: &LinkedNode) -> Option<()> {
        if !matches!(node.kind(), SyntaxKind::Star) {
            return None;
        }

        while !matches!(node.kind(), SyntaxKind::ModuleImport) {
            node = node.parent()?;
        }

        let import_node = node.cast::<ast::ModuleImport>()?;
        let scope_val = self
            .ctx
            .module_by_syntax(import_node.source().to_untyped())?;

        let scope_items = scope_val.scope()?.iter();
        let mut names = scope_items.map(|item| item.0.as_str()).collect::<Vec<_>>();
        names.sort();

        let content = format!("This star imports {}", separated_list(&names, "and"));
        self.def.push(content);
        Some(())
    }

    fn link(&mut self, mut node: &LinkedNode) -> Option<()> {
        while !matches!(node.kind(), SyntaxKind::FuncCall) {
            node = node.parent()?;
        }

        let links = get_link_exprs_in(node)?;
        let links = links
            .objects
            .iter()
            .filter(|link| link.range.contains(&self.cursor))
            .collect::<Vec<_>>();
        if links.is_empty() {
            return None;
        }

        for obj in links {
            let Some(target) = obj.target.resolve(self.ctx) else {
                continue;
            };
            // open file in tab or system application
            self.actions.push(CommandLink {
                title: Some("Open in Tab".to_string()),
                command_or_links: vec![CommandOrLink::Command {
                    id: "tinymist.openInternal".to_string(),
                    args: vec![JsonValue::String(target.to_string())],
                }],
            });
            self.actions.push(CommandLink {
                title: Some("Open Externally".to_string()),
                command_or_links: vec![CommandOrLink::Command {
                    id: "tinymist.openExternal".to_string(),
                    args: vec![JsonValue::String(target.to_string())],
                }],
            });
            if let Some(kind) = PathPreference::from_ext(target.path()) {
                self.def.push(format!("A `{kind:?}` file."));
            }
        }

        Some(())
    }

    fn preview(&mut self) -> Option<()> {
        // Preview results
        let provider = self.ctx.analysis.periscope.clone()?;
        let doc = self.doc.as_ref()?;
        let jump = |cursor| {
            jump_from_cursor(doc, &self.source, cursor)
                .into_iter()
                .next()
        };
        let position = jump(self.cursor);
        let position = position.or_else(|| {
            for idx in 1..100 {
                let next_cursor = self.cursor + idx;
                if next_cursor < self.source.text().len() {
                    let position = jump(next_cursor);
                    if position.is_some() {
                        return position;
                    }
                }
                let prev_cursor = self.cursor.checked_sub(idx);
                if let Some(prev_cursor) = prev_cursor {
                    let position = jump(prev_cursor);
                    if position.is_some() {
                        return position;
                    }
                }
            }

            None
        });

        log::info!("telescope position: {position:?}");

        let preview_content = provider.periscope_at(self.ctx, doc, position?)?;
        self.preview.push(preview_content);
        Some(())
    }
}

fn try_get_bib_details(
    doc: &Option<TypstDocument>,
    ctx: &LocalContext,
    name: &str,
) -> Option<RenderedBibCitation> {
    let doc = doc.as_ref()?;
    let support_html = !ctx.shared.analysis.remove_html;
    let bib_info = ctx.analyze_bib(doc.introspector())?;
    render_citation_string(&bib_info, name, support_html)
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
                Repr::Closure(..) | Repr::Plugin(..) => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("hover", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let docs = find_module_level_docs(&source).unwrap_or_default();
            let properties = get_test_properties(&docs);
            let graph = compile_doc_for_test(ctx, &properties);

            let request = HoverRequest {
                path: path.clone(),
                position: find_test_position(&source),
            };

            let result = request.request(ctx, graph);
            assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
        });
    }
}
