use std::fmt::Write;
use tinymist_analysis::{analyze_expr, upstream::truncated_repr_};
use typst::{engine::Sink, syntax::LinkedNode};
use typst_shim::syntax::LinkedNodeExt;

use crate::prelude::*;

/// A request to show the full tracked value at a specific position.
#[derive(Debug, Clone)]
pub struct ShowFullValueRequest {
    /// The source file.
    pub path: PathBuf,
    /// The cursor position.
    pub position: LspPosition,
}

impl StatefulRequest for ShowFullValueRequest {
    type Response = String;

    fn request(self, ctx: &mut LocalContext, _graph: LspComputeGraph) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let offset = ctx.to_typst_pos(self.position, &source)?;
        // the typst's cursor is 1-based, so we need to add 1 to the offset
        let cursor = offset + 1;

        let leaf = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;

        let tooltip = expr_tooltip(&ctx.world(), &leaf)?;

        Some(tooltip)
    }
}

fn expr_tooltip(world: &dyn World, leaf: &LinkedNode) -> Option<String> {
    let mut ancestor = leaf;
    while !ancestor.is::<ast::Expr>() {
        ancestor = ancestor.parent()?;
    }

    let expr = ancestor.cast::<ast::Expr>()?;
    if !expr.hash() && !matches!(expr, ast::Expr::MathIdent(_)) {
        return None;
    }

    let values = analyze_expr(world, ancestor);

    struct Piece<'a> {
        value: &'a Value,
        #[allow(unused)]
        first_occur: usize,
        count: usize,
    }

    let mut pieces: Vec<Piece<'_>> = vec![];
    let mut last = None;
    for (i, (value, _)) in values.iter().enumerate() {
        if last.replace(value).is_some_and(|last| *last == *value) {
            pieces.last_mut().unwrap().count += 1;
        } else {
            pieces.push(Piece {
                value,
                first_occur: i,
                count: 1,
            });
        }
    }

    const SIZE_LIMIT: usize = 512 * 1024 * 1024 * 1024; // 512MB

    let mut buf = String::new();
    let mut limited = false;
    for piece in pieces {
        let item_repr = truncated_repr_::<{ SIZE_LIMIT }>(piece.value);
        if buf.len() + item_repr.len() + 50 > SIZE_LIMIT {
            buf.push_str("... (reached size limit)\n");
            limited = true;
            break;
        }
        buf.push('#');
        buf.push_str(&item_repr);
        if piece.count > 1 {
            write!(buf, " // (x{})", piece.count).unwrap();
        }
        buf.push('\n');
    }
    if !limited && values.len() == Sink::MAX_VALUES {
        buf.push_str("... (reached max values limit)\n");
    }

    Some(buf)
}
