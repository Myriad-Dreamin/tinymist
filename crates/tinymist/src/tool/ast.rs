//! AST introspection tool.

use core::fmt;
use std::ops::Range;

use typst::syntax::LinkedNode;

pub(crate) struct AstRepr<'a>(pub LinkedNode<'a>, pub Option<Range<usize>>);

impl AstRepr<'_> {
    fn contains(&self, node: &LinkedNode) -> bool {
        let rng = self.1.as_ref();
        rng.is_some_and(|rng| {
            if rng.start == rng.end {
                return node.range().start == rng.start && node.range().end == rng.start
                    || node.range().start < rng.start && rng.start <= node.range().end;
            }

            !(rng.end <= node.range().start || rng.start >= node.range().end)
        })
    }

    fn node(&self, node: &LinkedNode, f: &mut fmt::Formatter<'_>, indent: usize) -> fmt::Result {
        if !self.contains(node) {
            return Ok(());
        }

        write!(f, "{: >indent$}{:?}(", "", node.kind())?;

        if !node.text().is_empty() {
            write!(f, "{:?}", node.text())?;
        } else if node.get().children().len() > 0 {
            write!(f, "{:?}, ", node.children().len())?;
            f.write_str("{\n")?;
            for child in node.children() {
                if !self.contains(&child) {
                    continue;
                }
                self.node(&child, f, indent + 1)?;
                f.write_str("\n")?;
            }
            write!(f, "{: >indent$}}}", "")?;
        }
        f.write_str(")")
    }
}

impl fmt::Display for AstRepr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("#")?;
        self.node(&self.0, f, 0)
    }
}
