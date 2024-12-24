use crate::prelude::*;

pub fn find_module_level_docs(src: &Source) -> Option<String> {
    crate::log_debug_ct!("finding docs at: {id:?}", id = src.id());

    let root = LinkedNode::new(src.root());
    for n in root.children() {
        if n.kind().is_trivia() {
            continue;
        }

        return extract_mod_docs_between(&root, 0..n.offset(), true);
    }

    extract_mod_docs_between(&root, 0..src.text().len(), true)
}

fn extract_mod_docs_between(
    node: &LinkedNode,
    rng: Range<usize>,
    first_group: bool,
) -> Option<String> {
    let mut matcher = DocCommentMatcher {
        strict: true,
        ..Default::default()
    };
    let nodes = node.children();
    'scan_comments: for n in nodes {
        let offset = n.offset();
        if offset < rng.start {
            continue 'scan_comments;
        }
        if offset >= rng.end {
            break 'scan_comments;
        }

        crate::log_debug_ct!("found comment for docs: {:?}: {:?}", n.kind(), n.text());
        if matcher.process(n.get()) {
            if first_group {
                break 'scan_comments;
            }
            matcher.comments.clear();
        }
    }

    matcher.collect()
}

pub enum CommentGroupSignal {
    Hash,
    Space,
    LineComment,
    BlockComment,
    BreakGroup,
}

#[derive(Default)]
pub struct CommentGroupMatcher {
    newline_count: u32,
}

impl CommentGroupMatcher {
    pub fn process(&mut self, n: &SyntaxNode) -> CommentGroupSignal {
        match n.kind() {
            SyntaxKind::Hash => {
                self.newline_count = 0;

                CommentGroupSignal::Hash
            }
            SyntaxKind::Space => {
                if n.text().contains('\n') {
                    self.newline_count += 1;
                }
                if self.newline_count > 1 {
                    return CommentGroupSignal::BreakGroup;
                }

                CommentGroupSignal::Space
            }
            SyntaxKind::Parbreak => {
                self.newline_count = 2;
                CommentGroupSignal::BreakGroup
            }
            SyntaxKind::LineComment => {
                self.newline_count = 0;
                CommentGroupSignal::LineComment
            }
            SyntaxKind::BlockComment => {
                self.newline_count = 0;
                CommentGroupSignal::BlockComment
            }
            _ => {
                self.newline_count = 0;
                CommentGroupSignal::BreakGroup
            }
        }
    }

    pub fn reset(&mut self) {
        self.newline_count = 0;
    }
}
enum RawComment {
    Line(EcoString),
    Block(EcoString),
}

#[derive(Default)]
pub struct DocCommentMatcher {
    comments: Vec<RawComment>,
    group_matcher: CommentGroupMatcher,
    strict: bool,
}

impl DocCommentMatcher {
    pub fn process(&mut self, n: &SyntaxNode) -> bool {
        match self.group_matcher.process(n) {
            CommentGroupSignal::LineComment => {
                let text = n.text();
                if !self.strict || text.starts_with("///") {
                    self.comments.push(RawComment::Line(text.clone()));
                }
            }
            CommentGroupSignal::BlockComment => {
                let text = n.text();
                if !self.strict {
                    self.comments.push(RawComment::Block(text.clone()));
                }
            }
            CommentGroupSignal::BreakGroup => {
                return true;
            }
            CommentGroupSignal::Hash | CommentGroupSignal::Space => {}
        }

        false
    }

    pub fn collect(&mut self) -> Option<String> {
        let comments = &self.comments;
        if comments.is_empty() {
            return None;
        }

        let comments = comments.iter().map(|comment| match comment {
            RawComment::Line(line) => {
                // strip all slash prefix
                let text = line.trim_start_matches('/');
                text
            }
            RawComment::Block(block) => {
                fn remove_comment(text: &str) -> Option<&str> {
                    let mut text = text.strip_prefix("/*")?.strip_suffix("*/")?.trim();
                    // trip start star
                    if text.starts_with('*') {
                        text = text.strip_prefix('*')?.trim();
                    }
                    Some(text)
                }

                remove_comment(block).unwrap_or(block.as_str())
            }
        });
        let comments = comments.collect::<Vec<_>>();

        let dedent = comments.iter().fold(usize::MAX, |acc, content| {
            let indent = content.chars().take_while(|ch| ch.is_whitespace()).count();
            acc.min(indent)
        });

        let size_hint = comments.iter().map(|comment| comment.len()).sum::<usize>();
        let mut comments = comments
            .iter()
            .map(|comment| comment.chars().skip(dedent).collect::<String>());

        let res = comments.try_fold(String::with_capacity(size_hint), |mut acc, comment| {
            if !acc.is_empty() {
                acc.push('\n');
            }

            acc.push_str(&comment);
            Some(acc)
        });

        self.comments.clear();
        res
    }

    pub fn reset(&mut self) {
        self.comments.clear();
        self.group_matcher.reset();
    }
}
