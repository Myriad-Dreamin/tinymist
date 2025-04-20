//! Convenient utilities to match comment in code.

use crate::prelude::*;

/// Extract the module-level documentation from a source.
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

/// A signal raised by the comment group matcher.
pub enum CommentGroupSignal {
    /// A hash marker is found.
    Hash,
    /// A space is found.
    Space,
    /// A line comment is found.
    LineComment,
    /// A block comment is found.
    BlockComment,
    /// The comment group should be broken.
    BreakGroup,
}

/// A matcher that groups comments.
#[derive(Default)]
pub struct CommentGroupMatcher {
    newline_count: u32,
}

impl CommentGroupMatcher {
    /// Reset the matcher. This usually happens after a group is collected or
    /// when some other child item is breaking the comment group manually.
    pub fn reset(&mut self) {
        self.newline_count = 0;
    }

    /// Process a child relative to some [`SyntaxNode`].
    ///
    /// ## Example
    ///
    /// See [`DocCommentMatcher`] for a real-world example.
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
}
enum RawComment {
    Line(EcoString),
    Block(EcoString),
}

/// A matcher that collects documentation comments.
#[derive(Default)]
pub struct DocCommentMatcher {
    comments: Vec<RawComment>,
    group_matcher: CommentGroupMatcher,
    strict: bool,
}

impl DocCommentMatcher {
    /// Reset the matcher. This usually happens after a group is collected or
    /// when some other child item is breaking the comment group manually.
    pub fn reset(&mut self) {
        self.comments.clear();
        self.group_matcher.reset();
    }

    /// Process a child relative to some [`SyntaxNode`].
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

    /// Collect the comments and return the result.
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

        let dedent = comments
            .iter()
            .flat_map(|line| {
                let mut chars = line.chars();
                let cnt = chars.by_ref().take_while(|c| c.is_whitespace()).count();
                chars.next().map(|_| cnt)
            })
            .min()
            .unwrap_or(0);

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() {
        let src = Source::detached(
            r#"/// foo
/// bar
#let main() = printf("hello World")"#,
        );
        let docs = find_module_level_docs(&src);
        assert_eq!(docs, Some("foo\nbar".to_string()));
    }

    #[test]
    fn issue_1687_postive() {
        let src = Source::detached(
            r#"/// Description.
/// 
/// Note.
#let main() = printf("hello World")"#,
        );
        let docs = find_module_level_docs(&src);
        assert_eq!(docs, Some("Description.\n\nNote.".to_string()));
    }

    #[test]
    fn issue_1687_negative() {
        let src = Source::detached(
            r#"/// Description.
///
/// Note.
#let main() = printf("hello World")"#,
        );
        let docs = find_module_level_docs(&src);
        assert_eq!(docs, Some("Description.\n\nNote.".to_string()));
    }
}
