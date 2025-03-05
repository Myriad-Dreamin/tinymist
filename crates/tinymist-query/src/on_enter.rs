//! <https://github.com/rust-lang/rust-analyzer/blob/master/docs/dev/lsp-extensions.md#on-enter>

use typst_shim::syntax::LinkedNodeExt;

use crate::{prelude::*, syntax::node_ancestors, SyntaxRequest};

/// The [`experimental/onEnter`] request is sent from client to server to handle
/// the <kbd>Enter</kbd> key press.
///
/// - `kbd:Enter` inside triple-slash comments automatically inserts `///`
/// - `kbd:Enter` in the middle or after a trailing space in `//` inserts `//`
/// - `kbd:Enter` inside `//!` doc comments automatically inserts `//!`
/// - `kbd:Enter` inside block math automatically inserts a newline and indents
/// - `kbd:Enter` inside `list` or `enum` items automatically automatically
///   inserts `-` or `+` and indents
///
/// [`experimental/onEnter`]: https://github.com/rust-lang/rust-analyzer/blob/master/docs/dev/lsp-extensions.md#on-enter
///
/// # Compatibility
///
/// This request was introduced in specification version 3.10.0.
#[derive(Debug, Clone)]
pub struct OnEnterRequest {
    /// The path of the document to get folding ranges for.
    pub path: PathBuf,
    /// The source code range to request for.
    pub range: LspRange,
}

impl SyntaxRequest for OnEnterRequest {
    type Response = Vec<TextEdit>;

    fn request(
        self,
        source: &Source,
        position_encoding: PositionEncoding,
    ) -> Option<Self::Response> {
        let root = LinkedNode::new(source.root());
        let rng = to_typst_range(self.range, position_encoding, source)?;
        let cursor = rng.start;
        let leaf = root.leaf_at_compat(cursor)?;

        let worker = OnEnterWorker {
            source,
            position_encoding,
        };

        enum Cases<'a> {
            LineComment(LinkedNode<'a>),
            Equation(LinkedNode<'a>),
            ListOrEnum(LinkedNode<'a>),
        }

        let case = node_ancestors(&leaf).find_map(|node| match node.kind() {
            SyntaxKind::LineComment => Some(Cases::LineComment(node.clone())),
            SyntaxKind::Equation => Some(Cases::Equation(node.clone())),
            SyntaxKind::ListItem | SyntaxKind::EnumItem => Some(Cases::ListOrEnum(node.clone())),
            SyntaxKind::Space | SyntaxKind::Parbreak => {
                let prev_leaf = node.prev_sibling()?;

                let inter_space = node.offset()..rng.start;
                if !inter_space.is_empty() && source.text()[inter_space].contains(['\r', '\n']) {
                    return None;
                }

                match prev_leaf.kind() {
                    SyntaxKind::ListItem | SyntaxKind::EnumItem => {
                        return Some(Cases::ListOrEnum(prev_leaf))
                    }
                    _ => {}
                }

                None
            }
            _ => None,
        });

        match case {
            Some(Cases::LineComment(node)) => worker.enter_line_doc_comment(node, rng),
            Some(Cases::Equation(node)) => worker.enter_block_math(node, rng),
            Some(Cases::ListOrEnum(node)) => worker.enter_list_or_enum(node, rng),
            _ => None,
        }
    }
}

struct OnEnterWorker<'a> {
    source: &'a Source,
    position_encoding: PositionEncoding,
}

impl OnEnterWorker<'_> {
    fn indent_of(&self, of: usize) -> String {
        let all_text = self.source.text();
        let start = all_text[..of].rfind('\n').map(|lf_offset| lf_offset + 1);
        let indent_size = all_text[start.unwrap_or_default()..of].chars().count();
        " ".repeat(indent_size)
    }

    fn enter_line_doc_comment(&self, leaf: LinkedNode, rng: Range<usize>) -> Option<Vec<TextEdit>> {
        let skipper = |n: &LinkedNode| {
            matches!(
                n.kind(),
                SyntaxKind::Space | SyntaxKind::Linebreak | SyntaxKind::LineComment
            )
        };
        let parent = leaf.parent()?;
        let till_curr = parent.children().take(leaf.index());
        let first_index = till_curr.rev().take_while(skipper).count();
        let comment_group_cnt = parent
            .children()
            .skip(leaf.index().saturating_sub(first_index))
            .take_while(skipper)
            .filter(|child| matches!(child.kind(), SyntaxKind::LineComment))
            .count();

        let comment_prefix = {
            let mut scanner = unscanny::Scanner::new(leaf.text());
            scanner.eat_while('/');
            scanner.eat_if('!');
            scanner.before()
        };

        // Continuing single-line non-doc comments (like this one :) ) is annoying
        if comment_group_cnt <= 1 && comment_prefix == "//" {
            return None;
        }

        let indent = self.indent_of(leaf.offset());
        // todo: remove_trailing_whitespace

        let edit = TextEdit {
            range: to_lsp_range(rng, self.source, self.position_encoding),
            new_text: format!("\n{indent}{comment_prefix} $0"),
        };

        Some(vec![edit])
    }

    fn enter_block_math(
        &self,
        math_node: LinkedNode<'_>,
        rng: Range<usize>,
    ) -> Option<Vec<TextEdit>> {
        let o = math_node.range();
        if !o.contains(&rng.end) {
            return None;
        }

        let all_text = self.source.text();
        let math_text = &all_text[o.clone()];
        let content = math_text.trim_start_matches('$').trim_end_matches('$');
        if !content.trim().is_empty() {
            return None;
        }

        let indent = self.indent_of(o.start);
        let edit = TextEdit {
            range: to_lsp_range(rng, self.source, self.position_encoding),
            // todo: read indent configuration
            new_text: if !content.contains('\n') {
                format!("\n{indent}  $0\n{indent}")
            } else {
                format!("\n{indent}  $0")
            },
        };

        Some(vec![edit])
    }

    fn enter_list_or_enum(&self, node: LinkedNode<'_>, rng: Range<usize>) -> Option<Vec<TextEdit>> {
        let indent = self.indent_of(node.range().start);

        let is_list = matches!(node.kind(), SyntaxKind::ListItem);
        let marker = if is_list { "-" } else { "+" };

        let edit = TextEdit {
            range: to_lsp_range(rng, self.source, self.position_encoding),
            new_text: format!("\n{indent}{marker} $0"),
        };

        Some(vec![edit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn prepare() {
        snapshot_testing("on_enter", &|world, path| {
            let source = world.source_by_path(&path).unwrap();

            let request = OnEnterRequest {
                path: path.clone(),
                range: find_test_range(&source),
            };

            let result = request.request(&source, PositionEncoding::Utf16);

            let annotated = {
                let range = find_test_range_(&source);
                let range_before = range.start.saturating_sub(10)..range.start;
                let range_window = range.clone();
                let range_after = range.end..range.end.saturating_add(10).min(source.text().len());

                let window_before = &source.text()[range_before];
                let window_line = &source.text()[range_window];
                let window_after = &source.text()[range_after];
                format!("{window_before}|{window_line}|{window_after}")
            };

            insta::with_settings!({
                description => format!("On Enter on {annotated})"),
            }, {
                assert_snapshot!(JsonRepr::new_redacted(result, &REDACT_LOC));
            })
        });
    }
}
