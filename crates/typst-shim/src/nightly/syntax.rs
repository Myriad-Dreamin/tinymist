//! Typst Syntax
use typst::syntax::LinkedNode;
use typst::syntax::Side;

/// The `LinkedNodeExt` trait is designed for compatibility between new and old versions of `typst`.
pub trait LinkedNodeExt: Sized {
    fn leaf_at_compat(&self, cursor: usize) -> Option<Self>;
}

impl LinkedNodeExt for LinkedNode {
    fn leaf_at_compat(&self, cursor: usize) -> Option<Self> {
        self.leaf_at(cursor, Side::Before)
    }
}
