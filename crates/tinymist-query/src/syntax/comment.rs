use std::ops::Range;

use crate::prelude::*;
use crate::syntax::get_def_target;

fn extract_document_between(node: &LinkedNode, rng: Range<usize>) -> Option<String> {
    // collect all comments before the definition
    let mut comments = vec![];

    let mut newline_count = 0;
    let nodes = node.parent()?.children();
    for n in nodes {
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
                    comments.clear();
                }
            }
            SyntaxKind::Parbreak => {
                newline_count = 2;
                comments.clear();
            }
            SyntaxKind::LineComment => {
                newline_count = 0;
                // comments.push(n.text().strip_prefix("//")?.trim().to_owned());
                // strip all slash prefix
                let text = n.text().trim_start_matches('/');
                comments.push(text.trim().to_owned());
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

    Some(comments.join("\n"))
}

pub fn find_document_before(src: &Source, cursor: usize) -> Option<String> {
    log::debug!("finding docs at: {id:?}, {cursor}", id = src.id());

    let root = LinkedNode::new(src.root());
    let leaf = root.leaf_at(cursor)?;
    let def_target = get_def_target(leaf.clone())?;
    log::info!("found docs target: {:?}", def_target.node().kind());
    // todo: import
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

        return extract_document_between(&node, start..end);
    }

    if node.parent()?.range() == root.range() && node.prev_sibling().is_none() {
        return extract_document_between(&node, root.offset()..node.range().start);
    }

    None
}
