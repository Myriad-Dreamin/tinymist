//! The [`CompileServerActor`] implementation borrowed from typst.ts.
//!
//! Please check `tinymist::actor::typ_client` for architecture details.

use std::num::NonZeroUsize;

use typst::model::Document;
use typst::{
    layout::{Frame, FrameItem, Point, Position},
    syntax::{LinkedNode, Source, Span, SyntaxKind},
};
use typst_shim::typst_linked_node_leaf_at;

/// Find the output location in the document for a cursor position.
pub fn jump_from_cursor(document: &Document, source: &Source, cursor: usize) -> Option<Position> {
    let node = typst_linked_node_leaf_at!(LinkedNode::new(source.root()), cursor)?;
    if node.kind() != SyntaxKind::Text {
        return None;
    }

    let mut min_dis = u64::MAX;
    let mut p = Point::default();
    let mut ppage = 0usize;

    let span = node.span();
    for (i, page) in document.pages.iter().enumerate() {
        let t_dis = min_dis;
        if let Some(pos) = find_in_frame(&page.frame, span, &mut min_dis, &mut p) {
            return Some(Position {
                page: NonZeroUsize::new(i + 1)?,
                point: pos,
            });
        }
        if t_dis != min_dis {
            ppage = i;
        }
    }

    if min_dis == u64::MAX {
        return None;
    }

    Some(Position {
        page: NonZeroUsize::new(ppage + 1)?,
        point: p,
    })
}

/// Find the position of a span in a frame.
fn find_in_frame(frame: &Frame, span: Span, min_dis: &mut u64, p: &mut Point) -> Option<Point> {
    for (mut pos, item) in frame.items() {
        if let FrameItem::Group(group) = item {
            // TODO: Handle transformation.
            if let Some(point) = find_in_frame(&group.frame, span, min_dis, p) {
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
                        *p = pos;
                    }
                }
                pos.x += glyph.x_advance.at(text.size);
            }
        }
    }

    None
}
