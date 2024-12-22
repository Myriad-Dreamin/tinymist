use typst_shim::syntax::LinkedNodeExt;

use crate::{prelude::*, SyntaxRequest};

/// The [`textDocument/selectionRange`] request is sent from the client to the
/// server to return suggested selection ranges at an array of given positions.
/// A selection range is a range around the cursor position which the user might
/// be interested in selecting.
///
/// [`textDocument/selectionRange`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_selectionRange
///
/// A selection range in the return array is for the position in the provided
/// parameters at the same index. Therefore `params.positions[i]` must be
/// contained in `result[i].range`.
///
/// # Compatibility
///
/// This request was introduced in specification version 3.15.0.
#[derive(Debug, Clone)]
pub struct SelectionRangeRequest {
    /// The path of the document to get selection ranges for.
    pub path: PathBuf,
    /// The positions to get selection ranges for.
    pub positions: Vec<LspPosition>,
}

impl SyntaxRequest for SelectionRangeRequest {
    type Response = Vec<SelectionRange>;

    fn request(
        self,
        source: &Source,
        position_encoding: PositionEncoding,
    ) -> Option<Self::Response> {
        let mut ranges = Vec::new();
        for position in self.positions {
            let typst_offset = to_typst_position(position, position_encoding, source)?;
            let tree = LinkedNode::new(source.root());
            let leaf = tree.leaf_at_compat(typst_offset + 1)?;
            ranges.push(range_for_node(source, position_encoding, &leaf));
        }

        Some(ranges)
    }
}

fn range_for_node(
    source: &Source,
    position_encoding: PositionEncoding,
    node: &LinkedNode,
) -> SelectionRange {
    let range = to_lsp_range(node.range(), source, position_encoding);
    SelectionRange {
        range,
        parent: node
            .parent()
            .map(|node| Box::new(range_for_node(source, position_encoding, node))),
    }
}
