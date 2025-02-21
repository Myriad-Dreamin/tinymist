//! Jumping from and to source and the rendered document.

use std::num::NonZeroUsize;

use tinymist_std::typst::TypstDocument;
use typst::{
    layout::{Frame, FrameItem, Point, Position},
    syntax::{LinkedNode, Source, Span, SyntaxKind},
};
use typst_shim::syntax::LinkedNodeExt;

/// Find the output location in the document for a cursor position.
pub fn jump_from_cursor(
    document: &TypstDocument,
    source: &Source,
    cursor: usize,
) -> Option<Position> {
    match document {
        TypstDocument::Paged(paged_doc) => {
            let node = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
            if node.kind() != SyntaxKind::Text {
                return None;
            }

            let mut min_dis = u64::MAX;
            let mut point = Point::default();
            let mut ppage = 0usize;

            let span = node.span();
            for (idx, page) in paged_doc.pages.iter().enumerate() {
                let t_dis = min_dis;
                if let Some(point) = find_in_frame(&page.frame, span, &mut min_dis, &mut point) {
                    return Some(Position {
                        page: NonZeroUsize::new(idx + 1)?,
                        point,
                    });
                }
                if t_dis != min_dis {
                    ppage = idx;
                }
            }

            if min_dis == u64::MAX {
                return None;
            }

            Some(Position {
                page: NonZeroUsize::new(ppage + 1)?,
                point,
            })
        }
    }
}

/// Find the position of a span in a frame.
fn find_in_frame(frame: &Frame, span: Span, min_dis: &mut u64, res: &mut Point) -> Option<Point> {
    for (mut pos, item) in frame.items() {
        if let FrameItem::Group(group) = item {
            // TODO: Handle transformation.
            if let Some(point) = find_in_frame(&group.frame, span, min_dis, res) {
                return Some(point + pos);
            }
        }

        if let FrameItem::Text(text) = item {
            for glyph in &text.glyphs {
                if glyph.span.0 == span {
                    return Some(pos);
                }
                if glyph.span.0.id() == span.id() {
                    let dis = glyph.span.0.number().abs_diff(span.number());
                    if dis < *min_dis {
                        *min_dis = dis;
                        *res = pos;
                    }
                }
                pos.x += glyph.x_advance.at(text.size);
            }
        }
    }

    None
}
