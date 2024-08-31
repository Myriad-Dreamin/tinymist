//! Typst Syntax
use typst::syntax::Side;

/// node.leaf_at(cursor)
#[macro_export]
macro_rules! typst_linked_node_leaf_at {
    ($node:expr, $cursor:expr) => {
        $node.leaf_at($cursor, Side::Before)
    };
}
