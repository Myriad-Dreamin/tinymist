use crate::prelude::*;

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

enum RawComment {
    Line(EcoString),
    Block(EcoString),
}

#[derive(Default)]
pub struct DocCommentMatcher {
    comments: Vec<RawComment>,
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
                self.comments.push(RawComment::Line(n.text().clone()));
            }
            SyntaxKind::BlockComment => {
                self.newline_count = 0;
                self.comments.push(RawComment::Block(n.text().clone()));
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

        let comments = comments.iter().map(|c| match c {
            RawComment::Line(c) => {
                // strip all slash prefix
                let text = c.trim_start_matches('/');
                text
            }
            RawComment::Block(c) => {
                fn remove_comment(text: &str) -> Option<&str> {
                    let mut text = text.strip_prefix("/*")?.strip_suffix("*/")?.trim();
                    // trip start star
                    if text.starts_with('*') {
                        text = text.strip_prefix('*')?.trim();
                    }
                    Some(text)
                }

                remove_comment(c).unwrap_or(c.as_str())
            }
        });
        let comments = comments.collect::<Vec<_>>();

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
