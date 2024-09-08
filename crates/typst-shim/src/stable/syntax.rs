//! Typst Syntax
use typst_syntax::LinkedNode;

/// The `LinkedNodeExt` trait is designed for compatibility between new and old versions of `typst`.
pub trait LinkedNodeExt: Sized {
    /// Get the leaf at the specified byte offset.
    fn leaf_at_compat(&self, cursor: usize) -> Option<Self>;
}

impl<'a> LinkedNodeExt for LinkedNode<'a> {
    fn leaf_at_compat(&self, cursor: usize) -> Option<Self> {
        self.leaf_at(cursor)
    }
}
