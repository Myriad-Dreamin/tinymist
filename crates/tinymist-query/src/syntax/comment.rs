use std::ops::Range;

use typst::syntax::SyntaxNode;
use typst_shim::syntax::LinkedNodeExt;

use crate::prelude::*;
use crate::syntax::get_def_target;

use super::DefTarget;

#[derive(Default)]
pub struct DocCommentMatcher {
    comments: Vec<String>,
    newline_count: usize,
}

impl DocCommentMatcher {
    pub fn process(&mut self, n: &SyntaxNode) -> bool {
        match n.kind() {
            SyntaxKind::Hash => {
                self.newline_count = 0;
            }
            SyntaxKind::Space => {
                if n.text().contains('\n') {
                    self.newline_count += 1;
                }
                if self.newline_count > 1 {
                    return true;
                }
            }
            SyntaxKind::Parbreak => {
                self.newline_count = 2;
                return true;
            }
            SyntaxKind::LineComment => {
                self.newline_count = 0;
                // comments.push(n.text().strip_prefix("//")?.trim().to_owned());
                // strip all slash prefix
                let text = n.text().trim_start_matches('/');
                self.comments.push(text.to_owned());
            }
            SyntaxKind::BlockComment => {
                self.newline_count = 0;
                let text = n.text();
                let text = remove_comment(text).unwrap_or(text.as_str());

                fn remove_comment(text: &str) -> Option<&str> {
                    let mut text = text.strip_prefix("/*")?.strip_suffix("*/")?.trim();
                    // trip start star
                    if text.starts_with('*') {
                        text = text.strip_prefix('*')?.trim();
                    }
                    Some(text)
                }

                self.comments.push(text.to_owned());
            }
            _ => {
                self.newline_count = 0;
            }
        }

        false
    }

    pub fn collect(&mut self) -> Option<String> {
        let comments = &self.comments;
        if comments.is_empty() {
            return None;
        }

        let dedent = comments.iter().fold(usize::MAX, |acc, c| {
            let indent = c.chars().take_while(|c| c.is_whitespace()).count();
            acc.min(indent)
        });

        let size_hint = comments.iter().map(|c| c.len()).sum::<usize>();
        let mut comments = comments
            .iter()
            .map(|c| c.chars().skip(dedent).collect::<String>());

        let res = comments.try_fold(String::with_capacity(size_hint), |mut acc, c| {
            if !acc.is_empty() {
                acc.push('\n');
            }

            acc.push_str(&c);
            Some(acc)
        });

        self.comments.clear();
        res
    }

    pub(crate) fn reset(&mut self) {
        self.comments.clear();
        self.newline_count = 0;
    }
}

fn extract_document_between(
    node: &LinkedNode,
    rng: Range<usize>,
    first_group: bool,
) -> Option<String> {
    let mut matcher = DocCommentMatcher::default();
    let nodes = node.children();
    'scan_comments: for n in nodes {
        let offset = n.offset();
        if offset < rng.start {
            continue 'scan_comments;
        }
        if offset >= rng.end {
            break 'scan_comments;
        }

        log::debug!("found comment for docs: {:?}: {:?}", n.kind(), n.text());
        if matcher.process(n.get()) {
            if first_group {
                break 'scan_comments;
            }
            matcher.comments.clear();
        }
    }

    matcher.collect()
}

pub fn find_docs_before(src: &Source, cursor: usize) -> Option<String> {
    log::debug!("finding docs at: {id:?}, {cursor}", id = src.id());

    let root = LinkedNode::new(src.root());
    let leaf = root.leaf_at_compat(cursor)?;
    let def_target = get_def_target(leaf.clone())?;
    find_docs_of(src, def_target)
}

pub fn find_docs_of(src: &Source, def_target: DefTarget) -> Option<String> {
    let root = LinkedNode::new(src.root());
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
