use core::fmt;

use crate::{
    analysis::{analyze_dyn_signature, find_definition, DefinitionLink, Signature},
    jump_from_cursor,
    prelude::*,
    syntax::{find_docs_before, get_deref_target, LexicalKind, LexicalVarKind},
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
            .or_else(|| star_tooltip(ctx, &source, cursor));

        let contents = contents.or_else(|| {
            Some(typst_to_lsp::tooltip(
                &ctx.tooltip(doc_ref, &source, cursor)?,
            ))
        })?;

        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
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
                .join("\n---\n"),
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

fn star_tooltip(
    ctx: &mut AnalysisContext,
    source: &Source,
    cursor: usize,
) -> Option<HoverContents> {
    let leaf = LinkedNode::new(source.root()).leaf_at(cursor)?;

    if !matches!(leaf.kind(), SyntaxKind::Star) {
        return None;
    }

    let mut leaf = &leaf;
    while !matches!(leaf.kind(), SyntaxKind::ModuleImport) {
        leaf = leaf.parent()?;
    }

    let mi: ast::ModuleImport = leaf.cast()?;
    let source = mi.source();
    let module = ctx.analyze_import(&leaf.find(source.span())?);
    log::info!("star import: {source:?} => {:?}", module.is_some());

    let i = module?;
    let scope = i.scope()?;

    let mut results = vec![];

    let mut names = scope.iter().map(|(name, _)| name).collect::<Vec<_>>();
    names.sort();
    let mut items = String::new();
    items.push_str("let (");
    for name in &names {
        items.push_str(name);
        items.push_str(", ");
    }
    items.push_str(") = ..");

    results.push(MarkedString::LanguageString(LanguageString {
        language: "typc".to_owned(),
        value: items,
    }));

    Some(LspHoverContents::Array(results))
}

enum CommandOrLink {
    Link(String),
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
    let leaf = LinkedNode::new(source.root()).leaf_at(cursor)?;

    let deref_target = get_deref_target(leaf.clone(), cursor)?;

    let lnk = find_definition(ctx, source.clone(), document, deref_target.clone())?;

    let mut results = vec![];
    let mut actions = vec![];

    match lnk.kind {
        LexicalKind::Mod(_)
        | LexicalKind::Var(LexicalVarKind::LabelRef)
        | LexicalKind::Var(LexicalVarKind::ValRef)
        | LexicalKind::Block
        | LexicalKind::Heading(..) => None,
        LexicalKind::Var(LexicalVarKind::Label) => {
            results.push(MarkedString::String(format!("Label: {}\n", lnk.name)));
            if let Some(c) = lnk.value.as_ref() {
                let c = truncated_repr(c);
                results.push(MarkedString::String(format!("{c}")));
            }
            Some(LspHoverContents::Array(results))
        }
        LexicalKind::Var(LexicalVarKind::BibKey) => {
            results.push(MarkedString::String(format!("Bibliography: @{}", lnk.name)));

            Some(LspHoverContents::Array(results))
        }
        LexicalKind::Var(LexicalVarKind::Function) => {
            let sig = if let Some(Value::Func(func)) = &lnk.value {
                Some(analyze_dyn_signature(ctx, func.clone()))
            } else {
                None
            };
            results.push(MarkedString::LanguageString(LanguageString {
                language: "typc".to_owned(),
                value: format!(
                    "let {name}({params});",
                    name = lnk.name,
                    params = ParamTooltip(sig)
                ),
            }));

            if let Some(doc) = DocTooltip::get(ctx, &lnk) {
                results.push(MarkedString::String(doc));
            }

            if let Some(link) = ExternalDocLink::get(ctx, &lnk) {
                actions.push(link);
            }

            render_actions(&mut results, actions);
            Some(LspHoverContents::Array(results))
        }
        LexicalKind::Var(LexicalVarKind::Variable) => {
            let deref_node = deref_target.node();
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
                value: format!("let {name};", name = lnk.name),
            }));

            if let Some(doc) = DocTooltip::get(ctx, &lnk) {
                results.push(MarkedString::String(doc));
            }

            if let Some(link) = ExternalDocLink::get(ctx, &lnk) {
                actions.push(link);
            }

            render_actions(&mut results, actions);
            Some(LspHoverContents::Array(results))
        }
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
                })
                .collect::<Vec<_>>()
                .join(" ");
            format!("[{title}]({command_or_links})")
        })
        .collect::<Vec<_>>()
        .join("___");
    results.push(MarkedString::String(g));
}

// todo: hover with `with_stack`
struct ParamTooltip(Option<Signature>);

impl fmt::Display for ParamTooltip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(sig) = &self.0 else {
            return Ok(());
        };
        let mut is_first = true;
        let mut write_sep = |f: &mut fmt::Formatter<'_>| {
            if is_first {
                is_first = false;
                return Ok(());
            }
            f.write_str(", ")
        };

        let primary_sig = match sig {
            Signature::Primary(sig) => sig,
            Signature::Partial(sig) => {
                let _ = sig.with_stack;

                &sig.signature
            }
        };

        for p in &primary_sig.pos {
            write_sep(f)?;
            write!(f, "{}", p.name)?;
        }
        if let Some(rest) = &primary_sig.rest {
            write_sep(f)?;
            write!(f, "{}", rest.name)?;
        }

        if !primary_sig.named.is_empty() {
            let mut name_prints = vec![];
            for v in primary_sig.named.values() {
                name_prints.push((v.name.clone(), v.type_repr.clone()))
            }
            name_prints.sort();
            for (k, v) in name_prints {
                write_sep(f)?;
                let v = v.as_deref().unwrap_or("any");
                let mut v = v.trim();
                if v.starts_with('{') && v.ends_with('}') && v.len() > 30 {
                    v = "{ ... }"
                }
                if v.starts_with('`') && v.ends_with('`') && v.len() > 30 {
                    v = "raw"
                }
                if v.starts_with('[') && v.ends_with(']') && v.len() > 30 {
                    v = "content"
                }
                write!(f, "{k}: {v}")?;
            }
        }

        Ok(())
    }
}

struct ExternalDocLink;

impl ExternalDocLink {
    fn get(ctx: &mut AnalysisContext, lnk: &DefinitionLink) -> Option<CommandLink> {
        self::ExternalDocLink::get_inner(ctx, lnk)
    }

    fn get_inner(_ctx: &mut AnalysisContext, lnk: &DefinitionLink) -> Option<CommandLink> {
        if matches!(lnk.value, Some(Value::Func(..))) {
            if let Some(builtin) = Self::builtin_func_tooltip("https://typst.app/docs/", lnk) {
                return Some(builtin);
            }
        };

        lnk.value
            .as_ref()
            .and_then(|value| Self::builtin_value_tooltip("https://typst.app/docs/", value))
    }
}

impl ExternalDocLink {
    fn builtin_func_tooltip(base: &str, lnk: &DefinitionLink) -> Option<CommandLink> {
        let Some(Value::Func(func)) = &lnk.value else {
            return None;
        };

        use typst::foundations::func::Repr;
        let mut func = func;
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
    pub fn get(ctx: &mut AnalysisContext, lnk: &DefinitionLink) -> Option<String> {
        self::DocTooltip::get_inner(ctx, lnk).map(|s| "\n\n".to_owned() + &s)
    }

    fn get_inner(ctx: &mut AnalysisContext, lnk: &DefinitionLink) -> Option<String> {
        if matches!(lnk.value, Some(Value::Func(..))) {
            if let Some(builtin) = Self::builtin_func_tooltip(lnk) {
                return Some(plain_docs_sentence(builtin).into());
            }
        };

        let (fid, def_range) = lnk.def_at.clone()?;

        let src = ctx.source_by_id(fid).ok()?;
        find_docs_before(&src, def_range.start)
    }
}

impl DocTooltip {
    fn builtin_func_tooltip(lnk: &DefinitionLink) -> Option<&'_ str> {
        let Some(Value::Func(func)) = &lnk.value else {
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
