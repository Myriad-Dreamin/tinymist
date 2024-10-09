//! <https://github.com/rust-lang/rust-analyzer/blob/master/docs/dev/lsp-extensions.md#on-enter>

use typst_shim::syntax::LinkedNodeExt;

use crate::{prelude::*, syntax::node_ancestors, SyntaxRequest};

/// The [`experimental/onEnter`] request is sent from client to server to handle
/// the <kbd>Enter</kbd> key press.
///
/// - `kbd:Enter` inside triple-slash comments automatically inserts `///`
/// - `kbd:Enter` in the middle or after a trailing space in `//` inserts `//`
/// - `kbd:Enter` inside `//!` doc comments automatically inserts `//!`
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
    /// The source code position to request for.
    pub position: LspPosition,
}

impl SyntaxRequest for OnEnterRequest {
    type Response = Vec<TextEdit>;

    fn request(
        self,
        source: &Source,
        position_encoding: PositionEncoding,
    ) -> Option<Self::Response> {
        let root = LinkedNode::new(source.root());
        let cursor = lsp_to_typst::position(self.position, position_encoding, source)?;
        let leaf = root.leaf_at_compat(cursor)?;

        let worker = OnEnterWorker {
            source,
            position_encoding,
        };

        if matches!(leaf.kind(), SyntaxKind::LineComment) {
            return worker.enter_line_doc_comment(&leaf, cursor);
        }

        let math_node =
            node_ancestors(&leaf).find(|node| matches!(node.kind(), SyntaxKind::Equation));
        if let Some(mn) = math_node {
            return worker.enter_block_math(mn, cursor);
        }

        None
    }
}

struct OnEnterWorker<'a> {
    source: &'a Source,
    position_encoding: PositionEncoding,
}

impl OnEnterWorker<'_> {
    fn indent_of(&self, of: usize) -> String {
        let all_text = self.source.text();
        let start = all_text[..of].rfind('\n').map(|e| e + 1);
        let indent_size = all_text[start.unwrap_or_default()..of].chars().count();
        " ".repeat(indent_size)
    }

    fn enter_line_doc_comment(&self, leaf: &LinkedNode, cursor: usize) -> Option<Vec<TextEdit>> {
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
            .filter(|e| matches!(e.kind(), SyntaxKind::LineComment))
            .count();

        let comment_prefix = {
            let mut p = unscanny::Scanner::new(leaf.text());
            p.eat_while('/');
            p.eat_if('!');
            p.before()
        };

        // Continuing single-line non-doc comments (like this one :) ) is annoying
        if comment_group_cnt <= 1 && comment_prefix == "//" {
            return None;
        }

        let indent = self.indent_of(leaf.offset());
        // todo: remove_trailing_whitespace

        let rng = cursor..cursor;

        let edit = TextEdit {
            range: typst_to_lsp::range(rng, self.source, self.position_encoding),
            new_text: format!("\n{indent}{comment_prefix} $0"),
        };

        Some(vec![edit])
    }

    fn enter_block_math(&self, math_node: &LinkedNode<'_>, cursor: usize) -> Option<Vec<TextEdit>> {
        let o = math_node.range();
        if !o.contains(&cursor) {
            return None;
        }

        let all_text = self.source.text();
        let math_text = &all_text[o.clone()];
        let content = math_text.trim_start_matches('$').trim_end_matches('$');
        if !content.trim().is_empty() {
            return None;
        }

        let indent = self.indent_of(o.start);
        let rng = cursor..cursor;
        let edit = TextEdit {
            range: typst_to_lsp::range(rng, self.source, self.position_encoding),
            // todo: read indent configuration
            new_text: if !content.contains('\n') {
                format!("\n{indent}  $0\n{indent}")
            } else {
                format!("\n{indent}  $0")
            },
        };

        Some(vec![edit])
    }
}
