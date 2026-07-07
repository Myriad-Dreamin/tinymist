use std::path::Path;
use std::sync::Arc;

use ecow::{EcoString, eco_format};
use tinymist_analysis::upstream::resolve;
use tinymist_std::path::unix_slash;
use tinymist_world::diag::print_diagnostics_to_string;
use tinymist_world::vfs::WorkspaceResolver;
use tinymist_world::{
    DiagnosticFormat, EntryReader, EntryState, ShadowApi, SourceWorld, TaskInputs,
};
use typlite::{Format, TypliteFeat};
use typst::World;
use typst::diag::StrResult;
use typst::foundations::Bytes;
use typst::syntax::FileId;
use typst_shim::syntax::VirtualPathExt;

use crate::analysis::SharedContext;
use crate::docs::{DocText, DocTextKind};

pub(crate) fn convert_docs(
    ctx: &SharedContext,
    content: &DocText,
    source_fid: Option<FileId>,
) -> StrResult<EcoString> {
    let mut entry = ctx.world().entry_state();
    let import_context = source_fid.map(|fid| {
        let root = ctx.world().vfs().resolve_root(fid).ok().flatten();
        if let Some(root) = root {
            entry = EntryState::new_workspace(root);
        }

        let mut imports = Vec::new();
        if WorkspaceResolver::is_package_file(fid)
            && let typst::syntax::VirtualRoot::Package(pkg) = fid.root()
        {
            let pkg_spec = pkg.to_string();
            imports.push(format!("#import {pkg_spec:?}"));
            imports.push(format!("#import {pkg_spec:?}: *"));
        }
        imports.push(format!(
            "#import {:?}: *",
            unix_slash(fid.vpath().as_rooted_path_compat())
        ));
        imports.join("; ")
    });
    let mut feat = TypliteFeat {
        color_theme: Some(ctx.analysis.color_theme),
        annotate_elem: true,
        soft_error: true,
        remove_html: ctx.analysis.remove_html,
        import_context,
        ..Default::default()
    };

    let entry = entry.select_in_workspace(Path::new("__tinymist_docs__.typ"));

    let mut w = ctx.world().task(TaskInputs {
        entry: Some(entry),
        inputs: None,
    });

    let content = prepare_docs_content(content);

    // todo: bad performance: content.to_owned()
    w.map_shadow_by_id(w.main(), Bytes::from_string(content))?;
    // todo: bad performance
    w.take_db();
    let (w, wrap_info) = feat
        .prepare_world(&w, Format::Md)
        .map_err(|e| eco_format!("failed to prepare world: {e}"))?;

    feat.wrap_info = wrap_info;

    let w = Arc::new(w);
    let res = typlite::Typlite::new(w.clone())
        .with_feature(feat)
        .convert();
    let conv = print_diag_or_error(w.as_ref(), res)?;

    Ok(conv.replace("```example", "```typ"))
}

fn prepare_docs_content(content: &DocText) -> String {
    if content.kind() != DocTextKind::Official {
        return content.raw().to_string();
    }

    let mut prepared = String::new();
    prepared.push_str("#let short-or-long(short, long) = long\n");
    prepared.push_str("#let docs-table(..args) = {\n");
    prepared.push_str("  let cells = args.pos()\n");
    prepared.push_str("  if cells.len() == 0 { return none }\n");
    prepared.push_str("  table(columns: cells.first().children.len(), ..cells)\n");
    prepared.push_str("}\n");
    prepared.push_str("#let __tinymist-docs-example = example\n");
    prepared.push_str("#let example(..args) = {\n");
    prepared.push_str("  let positional = args.pos()\n");
    prepared.push_str("  if positional.len() > 0 { __tinymist-docs-example(positional.at(0)) }\n");
    prepared.push_str("}\n");
    prepared.push_str("#let footnote(body) = [(#body)]\n");
    prepared.push('\n');
    prepared.push_str(&rewrite_builtin_doc_refs(content.raw()));
    prepared
}

fn rewrite_builtin_doc_refs(content: &str) -> String {
    let mut rewritten = String::with_capacity(content.len());
    let mut cursor = 0;

    while cursor < content.len() {
        if content[cursor..].starts_with("```") {
            let end = content[cursor + 3..]
                .find("```")
                .map(|offset| cursor + 3 + offset + 3)
                .unwrap_or(content.len());
            rewritten.push_str(&content[cursor..end]);
            cursor = end;
            continue;
        }

        let Some(offset) = content[cursor..].find('@') else {
            rewritten.push_str(&content[cursor..]);
            break;
        };

        let at = cursor + offset;
        rewritten.push_str(&content[cursor..at]);

        let start = at + 1;
        let Some(first) = content[start..].chars().next() else {
            rewritten.push('@');
            break;
        };

        if !is_ref_label_start(first) {
            rewritten.push('@');
            cursor = start;
            continue;
        }

        let mut end = start + first.len_utf8();
        for ch in content[end..].chars() {
            if !is_ref_label_continue(ch) {
                break;
            }

            end += ch.len_utf8();
        }

        let mut label_end = end;
        while label_end > start && content[..label_end].ends_with('.') {
            label_end -= 1;
        }

        let label = &content[start..label_end];
        if label.is_empty() {
            rewritten.push('@');
            cursor = start;
            continue;
        }

        let url = resolve(label, "https://typst.app/docs/").ok();
        if let Some((body, body_end)) = bracket_body(content, end) {
            push_link_or_text(&mut rewritten, url.as_deref(), body);
            cursor = body_end;
        } else {
            push_link_or_text(&mut rewritten, url.as_deref(), label);
            rewritten.push_str(&content[label_end..end]);
            cursor = end;
        }
    }

    rewritten
}

fn push_link_or_text(output: &mut String, url: Option<&str>, body: &str) {
    if let Some(url) = url {
        output.push_str("#link(");
        output.push_str(&format!("{url:?}"));
        output.push_str(")[");
        output.push_str(body);
        output.push(']');
    } else {
        output.push_str(body);
    }
}

fn bracket_body(content: &str, cursor: usize) -> Option<(&str, usize)> {
    if !content[cursor..].starts_with('[') {
        return None;
    }

    let body_start = cursor + 1;
    let mut depth = 1usize;
    let mut pos = body_start;
    for ch in content[body_start..].chars() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some((&content[body_start..pos], pos + 1));
                }
            }
            _ => {}
        }

        pos += ch.len_utf8();
    }

    None
}

fn is_ref_label_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || matches!(ch, '_' | '-')
}

fn is_ref_label_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':' | '/')
}

fn print_diag_or_error<T>(
    world: &impl SourceWorld,
    result: tinymist_std::Result<T>,
) -> StrResult<T> {
    match result {
        Ok(v) => Ok(v),
        Err(err) => {
            if let Some(diagnostics) = err.diagnostics() {
                return Err(print_diagnostics_to_string(
                    world,
                    diagnostics.iter(),
                    DiagnosticFormat::Human,
                )?);
            }

            Err(eco_format!("failed to convert docs: {err}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::*;

    use super::*;

    #[test]
    fn official_helpers_are_scoped() {
        run_with_sources("", |verse, path| {
            run_with_ctx(verse, path, &|ctx, _| {
                let docs = "#docs-table(table.header[A][B], [C], [D])";
                let shared = ctx.shared();

                let plain = DocText::plain(docs.into());
                let plain_err = convert_docs(shared, &plain, None).unwrap_err();
                assert!(plain_err.contains("unknown variable: docs-table"));

                let official_docs = DocText::official(docs.into());
                let official = convert_docs(shared, &official_docs, None).unwrap();
                assert!(official.contains("| A | B |"));
                assert!(official.contains("| C | D |"));
            });
        });
    }
}
