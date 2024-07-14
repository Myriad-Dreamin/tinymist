//! <https://github.com/rust-lang/rust-analyzer/blob/master/docs/dev/lsp-extensions.md#on-enter>

use crate::{prelude::*, SyntaxRequest};

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
        let leaf = root.leaf_at(cursor)?;

        let worker = OnEnterWorker {
            source,
            position_encoding,
        };

        if matches!(leaf.kind(), SyntaxKind::LineComment) {
            return worker.enter_line_doc_comment(&leaf, cursor);
        }

        None
    }
}

struct OnEnterWorker<'a> {
    source: &'a Source,
    position_encoding: PositionEncoding,
}

impl OnEnterWorker<'_> {
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

        // todo: indent
        let indent = "";
        // todo: remove_trailing_whitespace

        let rng = cursor..cursor;

        let edit = TextEdit {
            range: typst_to_lsp::range(rng, self.source, self.position_encoding),
            new_text: format!("\n{indent}{comment_prefix} $0"),
        };

        Some(vec![edit])
    }
}
