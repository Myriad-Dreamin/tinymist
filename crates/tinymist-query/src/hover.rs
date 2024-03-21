use core::fmt;

use crate::{
    analyze_signature, find_definition,
    prelude::*,
    syntax::{get_def_target, get_deref_target, LexicalVarKind},
    upstream::{expr_tooltip, tooltip, Tooltip},
    DefinitionLink, LspHoverContents, StatefulRequest,
};

#[derive(Debug, Clone)]
pub struct HoverRequest {
    pub path: PathBuf,
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
                ctx.world, doc, &source, cursor,
            )?))
        })?;

        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        let range = ctx.to_lsp_range(ast_node.range(), &source);

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

    let deref_target = get_deref_target(leaf.clone())?;

    let lnk = find_definition(ctx, source.clone(), deref_target.clone())?;

    match lnk.kind {
        crate::syntax::LexicalKind::Mod(_)
        | crate::syntax::LexicalKind::Var(LexicalVarKind::Label)
        | crate::syntax::LexicalKind::Var(LexicalVarKind::LabelRef)
        | crate::syntax::LexicalKind::Var(LexicalVarKind::ValRef)
        | crate::syntax::LexicalKind::Block
        | crate::syntax::LexicalKind::Heading(..) => None,
        crate::syntax::LexicalKind::Var(LexicalVarKind::Function) => Some(
            LspHoverContents::Scalar(lsp_types::MarkedString::String(format!(
                r#"```typc
let {name}({params});
```{doc}"#,
                name = lnk.name,
                params = ParamTooltip(&lnk),
                doc = DocTooltip::get(ctx, &lnk).unwrap_or_default(),
            ))),
        ),
        crate::syntax::LexicalKind::Var(LexicalVarKind::Variable) => {
            let deref_node = deref_target.node();
            // todo: check sensible length, value highlighting
            let values = expr_tooltip(ctx.world, deref_node)
                .map(|t| match t {
                    Tooltip::Text(s) => format!("// Values: {}", s),
                    Tooltip::Code(s) => format!("// Values: {}", s),
                })
                .unwrap_or_default();
            Some(LspHoverContents::Scalar(lsp_types::MarkedString::String(
                format!(
                    r#"```typc
{values}
let {name};
```{doc}"#,
                    name = lnk.name,
                    doc = DocTooltip::get(ctx, &lnk).unwrap_or_default(),
                ),
            )))
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
                name_prints.push((v.name.clone(), v.expr.clone()))
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
        // let doc = find_document_before(ctx, &lnk);

        if matches!(lnk.value, Some(Value::Func(..))) {
            if let Some(builtin) = Self::builtin_func_tooltip(lnk) {
                return Some(builtin.to_owned());
            }
        };

        Self::find_document_before(ctx, lnk)
    }
}

impl DocTooltip {
    fn find_document_before(ctx: &mut AnalysisContext, lnk: &DefinitionLink) -> Option<String> {
        log::debug!("finding comment at: {:?}, {}", lnk.fid, lnk.def_range.start);
        let src = ctx.source_by_id(lnk.fid).ok()?;
        let root = LinkedNode::new(src.root());
        let leaf = root.leaf_at(lnk.def_range.start + 1)?;
        let def_target = get_def_target(leaf.clone())?;
        log::info!("found comment target: {:?}", def_target.node().kind());
        // collect all comments before the definition
        let mut comments = vec![];
        // todo: import
        let target = def_target.node().clone();
        let mut node = def_target.node().clone();
        while let Some(prev) = node.prev_sibling() {
            node = prev;
            if node.kind() == SyntaxKind::Hash {
                continue;
            }

            let start = node.range().end;
            let end = target.range().start;

            if end <= start {
                break;
            }

            let nodes = node.parent()?.children();
            for n in nodes {
                let offset = n.offset();
                if offset > end || offset < start {
                    continue;
                }

                if n.kind() == SyntaxKind::Hash {
                    continue;
                }
                if n.kind() == SyntaxKind::LineComment {
                    // comments.push(n.text().strip_prefix("//")?.trim().to_owned());
                    // strip all slash prefix
                    let text = n.text().trim_start_matches('/');
                    comments.push(text.trim().to_owned());
                    continue;
                }
                if n.kind() == SyntaxKind::BlockComment {
                    let text = n.text();
                    let mut text = text.strip_prefix("/*")?.strip_suffix("*/")?.trim();
                    // trip start star
                    if text.starts_with('*') {
                        text = text.strip_prefix('*')?.trim();
                    }
                    comments.push(text.to_owned());
                }
            }
            break;
        }

        if comments.is_empty() {
            return None;
        }

        Some(comments.join("\n"))
    }

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
