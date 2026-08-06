//! Analyzing the syntax of a source file.
//!
//! This module must hide all **AST details** from the rest of the codebase.

pub(crate) mod docs;
mod expr;
pub(crate) mod index;
pub(crate) mod lexical_hierarchy;
pub(crate) mod module;

pub(crate) use expr::ExprRoute;
pub use index::*;
pub use lexical_hierarchy::*;
pub use module::*;
pub use tinymist_analysis::syntax::*;

#[cfg(test)]
mod matcher_tests {

    use typst::syntax::LinkedNode;
    use typst_shim::syntax::LinkedNodeExt;

    use crate::{syntax::classify_def, tests::*};

    #[test]
    fn test() {
        snapshot_testing_at("..", "match_def", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();

            let pos = ctx
                .to_typst_pos(find_test_position(&source), &source)
                .unwrap();

            let root = LinkedNode::new(source.root());
            let node = root.leaf_at_compat(pos).unwrap();

            let snap = classify_def(node).map(|def| format!("{:?}", def.node().range()));
            let snap = snap.as_deref().unwrap_or("<nil>");

            assert_snapshot!(snap);
        });
    }
}
