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

impl SemanticRequest for ShowFullValueRequest {
    type Response = String;

    fn request(self, ctx: &mut LocalContext) -> Option<Self::Response> {
        let source = ctx.source_by_path(&self.path).ok()?;
        let offset = ctx.to_typst_pos(self.position, &source)?;
        // the typst's cursor is 1-based, so we need to add 1 to the offset
        let cursor = offset + 1;

        let leaf = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
        let expr = get_inspected_expr(&leaf)?;
        let content = format_values(&ctx.world(), expr)?;

        Some(content)
    }
}

fn get_inspected_expr<'a>(leaf: &'a LinkedNode<'a>) -> Option<&'a LinkedNode<'a>> {
    let mut ancestor = leaf;

    // First, find the innermost expression containing the cursor
    while !ancestor.is::<ast::Expr>() {
        ancestor = ancestor.parent()?;
    }

    let expr = ancestor.cast::<ast::Expr>()?;

    // Don't inspect if it's an expression that's too broad
    // We want to skip block-level expressions and focus on leaf-like expressions
    if !expr.hash() && !matches!(expr, ast::Expr::MathIdent(_)) {
        return None;
    }

    // Try to find a more specific expression (child) if the current one is too broad
    // This helps with cases like array literals or parenthesized expressions
    if let Some(best_child) = find_best_child_expr(leaf, ancestor) {
        return Some(best_child);
    }

    Some(ancestor)
}

/// Try to find the best child expression of the ancestor that contains the leaf.
/// This helps narrow down the expression span when the ancestor is too broad.
fn find_best_child_expr<'a>(
    leaf: &'a LinkedNode<'a>,
    ancestor: &'a LinkedNode<'a>,
) -> Option<&'a LinkedNode<'a>> {
    let ancestor_span = ancestor.span();
    let mut current = leaf;
    let mut best_expr = None;

    // Walk up from leaf to ancestor, finding expressions on the way
    while let Some(parent) = current.parent() {
        // Check if we've reached the ancestor by comparing spans
        if parent.span() == ancestor_span {
            // Return the last expression we found before reaching the ancestor
            return best_expr;
        }

        // If parent is an expression, remember it
        if parent.is::<ast::Expr>() {
            best_expr = Some(parent);
        }
        current = parent;
    }

    None
}

fn format_values(world: &dyn World, expr: &LinkedNode) -> Option<String> {
    struct Piece<'a> {
        value: &'a Value,
        count: usize,
    }

    let values = analyze_expr(world, expr);

    let mut pieces: Vec<Piece<'_>> = vec![];
    let mut last = None;
    for (value, _) in values.iter() {
        if last.replace(value).is_some_and(|last| *last == *value) {
            pieces.last_mut().unwrap().count += 1;
        } else {
            pieces.push(Piece { value, count: 1 });
        }
    }

    const SIZE_LIMIT: usize = 8 * 1024 * 1024; // 8MB

    let mut buf = String::new();
    let mut value_limit_hit = false;
    let mut size_limit_hit = false;

    // Add header explaining limitations
    buf.push_str(&tinymist_l10n::t!(
        "tinymist-query.full-value.header",
        "# Tracked Values\n\n"
    ));

    for piece in pieces {
        let item_repr = truncated_repr_::<SIZE_LIMIT>(piece.value);
        if buf.len() + item_repr.len() + 50 > SIZE_LIMIT {
            buf.push_str(&tinymist_l10n::t!(
                "tinymist-query.full-value.size-limit",
                "... (reached size limit)\n"
            ));
            size_limit_hit = true;
            break;
        }
        buf.push('#');
        buf.push_str(&item_repr);
        if piece.count > 1 {
            write!(buf, " // (x{})", piece.count).unwrap();
        }
        buf.push('\n');
    }
    if !size_limit_hit && values.len() == Sink::MAX_VALUES {
        buf.push_str(&tinymist_l10n::t!(
            "tinymist-query.full-value.max-values-limit",
            "... (reached max values limit)\n"
        ));
        value_limit_hit = true;
    }

    // Add footer with limitation notes if any limits were hit
    if value_limit_hit || size_limit_hit {
        buf.push_str("\n\n");
        buf.push_str(&tinymist_l10n::t!(
            "tinymist-query.full-value.note-truncated",
            "**Note:** Values above may be truncated due to internal limits.\n"
        ));
    }

    Some(buf)
}
