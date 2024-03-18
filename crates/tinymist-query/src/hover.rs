use core::fmt;

use ecow::eco_format;

use crate::{
    analyze_signature, find_definition,
    prelude::*,
    syntax::{get_deref_target, LexicalVarKind},
    upstream::{expr_tooltip, tooltip, Tooltip},
    StatefulRequest,
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

        let typst_tooltip = def_tooltip(ctx, &source, cursor)
            .or_else(|| tooltip(ctx.world, doc, &source, cursor))?;

        let ast_node = LinkedNode::new(source.root()).leaf_at(cursor)?;
        let range = ctx.to_lsp_range(ast_node.range(), &source);

        Some(Hover {
            contents: typst_to_lsp::tooltip(&typst_tooltip),
            range: Some(range),
        })
    }
}

fn def_tooltip(ctx: &mut AnalysisContext, source: &Source, cursor: usize) -> Option<Tooltip> {
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
        crate::syntax::LexicalKind::Var(LexicalVarKind::Function) => {
            let f = lnk.value.as_ref();
            Some(Tooltip::Text(eco_format!(
                r#"```typc
let {}({});
```{}"#,
                lnk.name,
                ParamTooltip(f),
                DocTooltip(f),
            )))
        }
        crate::syntax::LexicalKind::Var(LexicalVarKind::Variable) => {
            let deref_node = deref_target.node();
            let v = expr_tooltip(ctx.world, deref_node)
                .map(|t| match t {
                    Tooltip::Text(s) => format!("\n\nValues: {}", s),
                    Tooltip::Code(s) => format!("\n\nValues: ```{}```", s),
                })
                .unwrap_or_default();

            Some(Tooltip::Text(eco_format!(
                r#"
```typc
let {};
```{v}"#,
                lnk.name
            )))
        }
    }
}

struct ParamTooltip<'a>(Option<&'a Value>);

impl<'a> fmt::Display for ParamTooltip<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(Value::Func(func)) = self.0 else {
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

struct DocTooltip<'a>(Option<&'a Value>);

impl<'a> fmt::Display for DocTooltip<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(Value::Func(func)) = self.0 else {
            return Ok(());
        };

        f.write_str("\n\n")?;

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
                    return Ok(());
                }
            }
        };

        f.write_str(docs)
    }
}
