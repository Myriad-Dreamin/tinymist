use std::ops::Range;

use typst_shim::syntax::LinkedNodeExt;

use crate::prelude::*;
use crate::syntax::get_def_target;

fn extract_document_between(
    node: &LinkedNode,
    rng: Range<usize>,
    first_group: bool,
) -> Option<String> {
    // collect all comments before the definition
    let mut comments = vec![];

    let mut newline_count = 0;
    let nodes = node.children();
    'scan_comments: for n in nodes {
        let offset = n.offset();
        if !rng.contains(&offset) {
            continue;
        }

        log::debug!("found comment for docs: {:?}: {:?}", n.kind(), n.text());

        match n.kind() {
            SyntaxKind::Hash => {
                newline_count = 0;
            }
            SyntaxKind::Space => {
                if n.text().contains('\n') {
                    newline_count += 1;
                }
                if newline_count > 1 {
                    if first_group {
                        break 'scan_comments;
                    }
                    comments.clear();
                }
            }
            SyntaxKind::Parbreak => {
                newline_count = 2;
                if first_group {
                    break 'scan_comments;
                }
                comments.clear();
            }
            SyntaxKind::LineComment => {
                newline_count = 0;
                // comments.push(n.text().strip_prefix("//")?.trim().to_owned());
                // strip all slash prefix
                let text = n.text().trim_start_matches('/');
                comments.push(text.to_owned());
                continue;
            }
            SyntaxKind::BlockComment => {
                newline_count = 0;
                let text = n.text();
                let mut text = text.strip_prefix("/*")?.strip_suffix("*/")?.trim();
                // trip start star
                if text.starts_with('*') {
                    text = text.strip_prefix('*')?.trim();
                }
                comments.push(text.to_owned());
            }
            _ => {
                newline_count = 0;
            }
        }
    }

    if comments.is_empty() {
        return None;
    }

    let dedent = comments.iter().fold(usize::MAX, |acc, c| {
        let indent = c.chars().take_while(|c| c.is_whitespace()).count();
        acc.min(indent)
    });

    let docs = comments
        .iter()
        .map(|c| c.chars().skip(dedent).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");

    Some(docs)
}

pub fn find_docs_before(src: &Source, cursor: usize) -> Option<String> {
    log::debug!("finding docs at: {id:?}, {cursor}", id = src.id());

    let root = LinkedNode::new(src.root());
    let leaf = root.leaf_at_compat(cursor)?;
    let def_target = get_def_target(leaf.clone())?;
    log::debug!("found docs target: {:?}", def_target.node().kind());
    // todo: import node
    let target = def_target.node().clone();
    let mut node = target.clone();
    while let Some(prev) = node.prev_sibling() {
        node = prev;
        if node.kind() == SyntaxKind::Hash {
            continue;
        }

        let start = node.range().end;
        let end = target.range().start;

        if end <= start {
            return None;
        }

        return extract_document_between(node.parent()?, start..end, false);
    }

    if node.parent()?.range() == root.range() && node.prev_sibling().is_none() {
        return extract_document_between(node.parent()?, root.offset()..node.range().start, false);
    }

    None
}

pub fn find_module_level_docs(src: &Source) -> Option<String> {
    log::debug!("finding docs at: {id:?}", id = src.id());

    let root = LinkedNode::new(src.root());
    for n in root.children() {
        if n.kind().is_trivia() {
            continue;
        }

        return extract_document_between(&root, 0..n.offset(), true);
    }

    extract_document_between(&root, 0..src.text().len(), true)
}
