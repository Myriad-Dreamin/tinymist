use core::fmt;

use typst_shim::syntax::LinkedNodeExt;

use crate::{
    analysis::{get_link_exprs_in, Definition},
    docs::SignatureDocs,
    jump_from_cursor,
    prelude::*,
    syntax::{find_docs_before, get_deref_target, Decl},
    ty::PathPreference,
    upstream::{expr_tooltip, plain_docs_sentence, route_of_value, truncated_repr, Tooltip},
    LspHoverContents, StatefulRequest,
};

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
        ctx: &mut AnalysisContext,
        doc: Option<VersionedDocument>,
    ) -> Option<Self::Response> {
        let doc_ref = doc.as_ref().map(|doc| doc.document.as_ref());

        let source = ctx.source_by_path(&self.path).ok()?;
        let offset = ctx.to_typst_pos(self.position, &source)?;
        // the typst's cursor is 1-based, so we need to add 1 to the offset
        let cursor = offset + 1;

        let contents = def_tooltip(ctx, &source, doc.as_ref(), cursor)
            .or_else(|| star_tooltip(ctx, &source, cursor))
            .or_else(|| link_tooltip(ctx, &source, cursor));

        let contents = contents.or_else(|| {
            Some(typst_to_lsp::tooltip(
                &ctx.tooltip(doc_ref, &source, cursor)?,
            ))
        })?;

        let ast_node = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
        let range = ctx.to_lsp_range(ast_node.range(), &source);

        // Neovim shows ugly hover if the hover content is in array, so we join them
        // manually with divider bars.
        let mut contents = match contents {
            LspHoverContents::Array(contents) => contents
                .into_iter()
                .map(|e| match e {
                    MarkedString::LanguageString(e) => {
                        format!("```{}\n{}\n```", e.language, e.value)
                    }
                    MarkedString::String(e) => e,
                })
                .join("\n\n---\n"),
            LspHoverContents::Scalar(MarkedString::String(contents)) => contents,
            LspHoverContents::Scalar(MarkedString::LanguageString(contents)) => {
                format!("```{}\n{}\n```", contents.language, contents.value)
            }
            lsp_types::HoverContents::Markup(e) => {
                match e.kind {
                    MarkupKind::Markdown => e.value,
                    // todo: escape
                    MarkupKind::PlainText => e.value,
                }
            }
        };

        if ctx.analysis.enable_periscope {
            if let Some(doc) = doc.clone() {
                let position = jump_from_cursor(&doc.document, &source, cursor);
                let position = position.or_else(|| {
                    for i in 1..100 {
                        let next_cursor = cursor + i;
                        if next_cursor < source.text().len() {
                            let position = jump_from_cursor(&doc.document, &source, next_cursor);
                            if position.is_some() {
                                return position;
                            }
                        }
                        let prev_cursor = cursor.checked_sub(i);
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
                let content = position.and_then(|pos| ctx.resources.periscope_at(ctx, doc, pos));
                if let Some(preview_content) = content {
                    contents = format!("{preview_content}\n---\n{contents}");
                }
            }
        }

        Some(Hover {
            contents: LspHoverContents::Scalar(MarkedString::String(contents)),
            range: Some(range),
        })
    }
}

fn link_tooltip(
    ctx: &mut AnalysisContext<'_>,
    source: &Source,
    cursor: usize,
) -> Option<HoverContents> {
    let mut node = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
    while !matches!(node.kind(), SyntaxKind::FuncCall) {
        node = node.parent()?.clone();
    }

    let mut links = get_link_exprs_in(ctx, &node)?;
    links.retain(|link| link.0.contains(&cursor));
    if links.is_empty() {
        return None;
    }

    let mut results = vec![];
    let mut actions = vec![];
    for (_, target) in links {
        // open file in tab or system application
        actions.push(CommandLink {
            title: Some("Open in Tab".to_string()),
            command_or_links: vec![CommandOrLink::Command(Command {
                id: "tinymist.openInternal".to_string(),
                args: vec![JsonValue::String(target.to_string())],
            })],
        });
        actions.push(CommandLink {
            title: Some("Open Externally".to_string()),
            command_or_links: vec![CommandOrLink::Command(Command {
                id: "tinymist.openExternal".to_string(),
                args: vec![JsonValue::String(target.to_string())],
            })],
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

    Some(LspHoverContents::Array(results))
}

fn star_tooltip(
    ctx: &mut AnalysisContext,
    source: &Source,
    cursor: usize,
) -> Option<HoverContents> {
    let leaf = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;

    if !matches!(leaf.kind(), SyntaxKind::Star) {
        return None;
    }

    let mut leaf = &leaf;
    while !matches!(leaf.kind(), SyntaxKind::ModuleImport) {
        leaf = leaf.parent()?;
    }

    let mi: ast::ModuleImport = leaf.cast()?;
    let source = mi.source();
    let module = ctx.analyze_import(source.to_untyped()).1;
    log::debug!("star import: {source:?} => {:?}", module.is_some());

    let i = module?;
    let scope = i.scope()?;

    let mut results = vec![];

    let mut names = scope.iter().map(|(name, _, _)| name).collect::<Vec<_>>();
    names.sort();
    let items = typst::foundations::repr::separated_list(&names, "and");

    results.push(MarkedString::String(format!("This star imports {items}")));
    Some(LspHoverContents::Array(results))
}

struct Command {
    id: String,
    args: Vec<JsonValue>,
}

enum CommandOrLink {
    Link(String),
    Command(Command),
}

struct CommandLink {
    title: Option<String>,
    command_or_links: Vec<CommandOrLink>,
}

fn def_tooltip(
    ctx: &mut AnalysisContext,
    source: &Source,
    document: Option<&VersionedDocument>,
    cursor: usize,
) -> Option<LspHoverContents> {
    let leaf = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;

    let deref_target = get_deref_target(leaf.clone(), cursor)?;

    let def = ctx.definition(source, document, deref_target.clone())?;

    let mut results = vec![];
    let mut actions = vec![];

    use Decl::*;
    match def.decl.as_ref() {
        Label(..) => {
            results.push(MarkedString::String(format!("Label: {}\n", def.name())));
            // todo: type repr
            if let Some(c) = def.term.as_ref().and_then(|v| v.value()) {
                let c = truncated_repr(&c);
                results.push(MarkedString::String(format!("{c}")));
            }
            Some(LspHoverContents::Array(results))
        }
        BibEntry(..) => {
            results.push(MarkedString::String(format!(
                "Bibliography: @{}",
                def.name()
            )));

            Some(LspHoverContents::Array(results))
        }
        Func(..) | Closure(..) => {
            let sig = ctx.signature_docs(&def);

            results.push(MarkedString::LanguageString(LanguageString {
                language: "typc".to_owned(),
                value: format!(
                    "let {name}({params}){result};",
                    name = def.name(),
                    params = ParamTooltip(sig.as_ref()),
                    result =
                        ResultTooltip(def.name(), sig.as_ref().and_then(|sig| sig.ret_ty.as_ref()))
                ),
            }));

            if let Some(doc) = sig {
                results.push(MarkedString::String(doc.def_docs().into()));
            }

            if let Some(link) = ExternalDocLink::get(ctx, &def) {
                actions.push(link);
            }

            render_actions(&mut results, actions);
            Some(LspHoverContents::Array(results))
        }
        PathStem(..) | Var(..) => {
            let deref_node = deref_target.node();
            let sig = ctx.variable_docs(deref_target.node());

            // todo: check sensible length, value highlighting
            if let Some(values) = expr_tooltip(ctx.world(), deref_node) {
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

            results.push(MarkedString::LanguageString(LanguageString {
                language: "typc".to_owned(),
                value: format!(
                    "let {name}{result};",
                    name = def.name(),
                    result = ResultTooltip(
                        def.name(),
                        sig.as_ref().and_then(|sig| sig.return_ty.as_ref())
                    )
                ),
            }));

            if let Some(doc) = sig {
                results.push(MarkedString::String(doc.def_docs().clone()));
            } else if let Some(doc) = DocTooltip::get(ctx, &def) {
                results.push(MarkedString::String(doc));
            }

            if let Some(link) = ExternalDocLink::get(ctx, &def) {
                actions.push(link);
            }

            render_actions(&mut results, actions);
            Some(LspHoverContents::Array(results))
        }
        Pattern(..) | Docs(..) | Generated(..) | ImportAlias(..) | Constant(..) | IdentRef(..)
        | ModuleAlias(..) | Module(..) | Import(..) | ContentRef(..) | StrName(..)
        | ModuleImport(..) | Content(..) | ImportPath(..) | IncludePath(..) | Spread(..) => None,
    }
}

fn render_actions(results: &mut Vec<MarkedString>, actions: Vec<CommandLink>) {
    if actions.is_empty() {
        return;
    }

    let g = actions
        .into_iter()
        .map(|action| {
            // https://github.com/rust-lang/rust-analyzer/blob/1a5bb27c018c947dab01ab70ffe1d267b0481a17/editors/code/src/client.ts#L59
            let title = action.title.unwrap_or("".to_owned());
            let command_or_links = action
                .command_or_links
                .into_iter()
                .map(|col| match col {
                    CommandOrLink::Link(link) => link,
                    CommandOrLink::Command(command) => {
                        let id = command.id;
                        // <https://code.visualstudio.com/api/extension-guides/command#command-uris>
                        if command.args.is_empty() {
                            format!("command:{id}")
                        } else {
                            let args = serde_json::to_string(&command.args).unwrap();
                            let args = percent_encoding::utf8_percent_encode(
                                &args,
                                percent_encoding::NON_ALPHANUMERIC,
                            );
                            format!("command:{id}?{args}")
                        }
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            format!("[{title}]({command_or_links})")
        })
        .collect::<Vec<_>>()
        .join(" | ");
    results.push(MarkedString::String(g));
}

// todo: hover with `with_stack`
struct ParamTooltip<'a>(Option<&'a SignatureDocs>);

impl fmt::Display for ParamTooltip<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(sig) = self.0 else {
            return Ok(());
        };
        sig.fmt(f)
    }
}

struct ResultTooltip<'a>(&'a str, Option<&'a (String, String)>);

impl fmt::Display for ResultTooltip<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some((short, _)) = self.1 else {
            return Ok(());
        };
        if short == self.0 {
            return Ok(());
        }

        write!(f, " = {short}")
    }
}

struct ExternalDocLink;

impl ExternalDocLink {
    fn get(ctx: &mut AnalysisContext, def: &Definition) -> Option<CommandLink> {
        self::ExternalDocLink::get_inner(ctx, def)
    }

    fn get_inner(_ctx: &mut AnalysisContext, def: &Definition) -> Option<CommandLink> {
        let value = def.value();

        if matches!(value, Some(Value::Func(..))) {
            if let Some(builtin) = Self::builtin_func_tooltip("https://typst.app/docs/", def) {
                return Some(builtin);
            }
        };

        value.and_then(|value| Self::builtin_value_tooltip("https://typst.app/docs/", &value))
    }
}

impl ExternalDocLink {
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

pub(crate) struct DocTooltip;

impl DocTooltip {
    pub fn get(ctx: &mut AnalysisContext, def: &Definition) -> Option<String> {
        self::DocTooltip::get_inner(ctx, def).map(|s| "\n\n".to_owned() + &s)
    }

    fn get_inner(ctx: &mut AnalysisContext, def: &Definition) -> Option<String> {
        let value = def.value();
        if matches!(value, Some(Value::Func(..))) {
            if let Some(builtin) = Self::builtin_func_tooltip(def) {
                return Some(plain_docs_sentence(builtin).into());
            }
        };

        let (fid, def_range) = def.def_at(ctx.shared()).clone()?;

        let src = ctx.source_by_id(fid).ok()?;
        find_docs_before(&src, def_range.start)
    }
}

impl DocTooltip {
    fn builtin_func_tooltip(def: &Definition) -> Option<&'_ str> {
        let value = def.value();
        let Some(Value::Func(func)) = &value else {
            return None;
        };

        use typst::foundations::func::Repr;
        let mut func = func;
        let docs = 'search: loop {
            match func.inner() {
                Repr::Native(n) => break 'search n.docs,
                Repr::Element(e) => break 'search e.docs(),
                Repr::With(w) => {
                    func = &w.0;
                }
                Repr::Closure(..) => {
                    return None;
                }
            }
        };

        Some(docs)
    }
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
