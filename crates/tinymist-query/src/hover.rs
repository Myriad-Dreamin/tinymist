use core::fmt;

use crate::{
    analysis::analyze_signature,
    find_definition,
    prelude::*,
    syntax::{find_document_before, get_deref_target, LexicalKind, LexicalVarKind},
    upstream::{expr_tooltip, tooltip, Tooltip},
    DefinitionLink, LspHoverContents, StatefulRequest,
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
        let doc = doc.as_ref().map(|doc| doc.document.as_ref());

        let source = ctx.source_by_path(&self.path).ok()?;
        let offset = ctx.to_typst_pos(self.position, &source)?;
        // the typst's cursor is 1-based, so we need to add 1 to the offset
        let cursor = offset + 1;

        let contents = def_tooltip(ctx, &source, cursor).or_else(|| {
            Some(typst_to_lsp::tooltip(&tooltip(
                ctx.world(),
                doc,
                &source,
                cursor,
            )?))
        })?;

        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        let range = ctx.to_lsp_range(ast_node.range(), &source);

        let contents = match contents {
            LspHoverContents::Array(contents) => LspHoverContents::Scalar(MarkedString::String(
                contents
                    .into_iter()
                    .map(|e| match e {
                        MarkedString::LanguageString(e) => {
                            format!("```{}\n{}\n```", e.language, e.value)
                        }
                        MarkedString::String(e) => e,
                    })
                    .join("\n---\n"),
            )),
            contents => contents,
        };

        Some(Hover {
            contents,
            range: Some(range),
        })
    }
}

fn def_tooltip(
    ctx: &mut AnalysisContext,
    source: &Source,
    cursor: usize,
) -> Option<LspHoverContents> {
    let leaf = LinkedNode::new(source.root()).leaf_at(cursor)?;

    let deref_target = get_deref_target(leaf.clone(), cursor)?;

    let lnk = find_definition(ctx, source.clone(), deref_target.clone())?;

    let mut results = vec![];

    match lnk.kind {
        LexicalKind::Mod(_)
        | LexicalKind::Var(LexicalVarKind::Label)
        | LexicalKind::Var(LexicalVarKind::LabelRef)
        | LexicalKind::Var(LexicalVarKind::ValRef)
        | LexicalKind::Block
        | LexicalKind::Heading(..) => None,
        LexicalKind::Var(LexicalVarKind::Function) => {
            results.push(MarkedString::LanguageString(LanguageString {
                language: "typc".to_owned(),
                value: format!(
                    "let {name}({params});",
                    name = lnk.name,
                    params = ParamTooltip(&lnk)
                ),
            }));

            if let Some(doc) = DocTooltip::get(ctx, &lnk) {
                results.push(MarkedString::String(doc));
            }

            Some(LspHoverContents::Array(results))
        }
        LexicalKind::Var(LexicalVarKind::Variable) => {
            let deref_node = deref_target.node();
            // todo: check sensible length, value highlighting
            if let Some(values) = expr_tooltip(ctx.world(), deref_node) {
                match values {
                    Tooltip::Text(values) => {
                        results.push(MarkedString::String(format!("Values: {values}")));
                    }
                    Tooltip::Code(values) => {
                        results.push(MarkedString::LanguageString(LanguageString {
                            language: "typc".to_owned(),
                            value: format!("// Values\n{values}"),
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

            Some(LspHoverContents::Array(results))
        }
    }
}

struct ParamTooltip<'a>(&'a DefinitionLink);

impl<'a> fmt::Display for ParamTooltip<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(Value::Func(func)) = &self.0.value else {
            return Ok(());
        };

        let sig = analyze_signature(func.clone());

        let mut is_first = true;
        let mut write_sep = |f: &mut fmt::Formatter<'_>| {
            if is_first {
                is_first = false;
                return Ok(());
            }
            f.write_str(", ")
        };
        for p in &sig.pos {
            write_sep(f)?;
            write!(f, "{}", p.name)?;
        }
        if let Some(rest) = &sig.rest {
            write_sep(f)?;
            write!(f, "{}", rest.name)?;
        }

        if !sig.named.is_empty() {
            let mut name_prints = vec![];
            for v in sig.named.values() {
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

struct DocTooltip;

impl DocTooltip {
    fn get(ctx: &mut AnalysisContext, lnk: &DefinitionLink) -> Option<String> {
        self::DocTooltip::get_inner(ctx, lnk).map(|s| "\n\n".to_owned() + &s)
    }

    fn get_inner(ctx: &mut AnalysisContext, lnk: &DefinitionLink) -> Option<String> {
        if matches!(lnk.value, Some(Value::Func(..))) {
            if let Some(builtin) = Self::builtin_func_tooltip(lnk) {
                return Some(builtin.to_owned());
            }
        };

        let src = ctx.source_by_id(lnk.fid).ok()?;
        find_document_before(&src, lnk.def_range.start)
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
