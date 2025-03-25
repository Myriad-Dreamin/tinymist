//! Jumping from and to source and the rendered document.

use std::num::NonZeroUsize;

use tinymist_project::LspWorld;
use tinymist_std::{debug_loc::SourceSpanOffset, typst::TypstDocument};
use typst::{
    layout::{Frame, FrameItem, Point, Position, Size},
    syntax::{LinkedNode, Source, Span, SyntaxKind},
    visualize::Geometry,
    World,
};
use typst_shim::syntax::LinkedNodeExt;

/// Finds a span range from a clicked physical position in a rendered paged
/// document.
pub fn jump_from_click(
    world: &LspWorld,
    frame: &Frame,
    click: Point,
) -> Option<(SourceSpanOffset, SourceSpanOffset)> {
    // Try to find a link first.
    for (pos, item) in frame.items() {
        if let FrameItem::Link(_dest, size) = item {
            if is_in_rect(*pos, *size, click) {
                // todo: url reaction
                return None;
            }
        }
    }

    // If there's no link, search for a jump target.
    for (mut pos, item) in frame.items().rev() {
        match item {
            FrameItem::Group(group) => {
                // TODO: Handle transformation.
                if let Some(span) = jump_from_click(world, &group.frame, click - pos) {
                    return Some(span);
                }
            }

            FrameItem::Text(text) => {
                for glyph in &text.glyphs {
                    let width = glyph.x_advance.at(text.size);
                    if is_in_rect(
                        Point::new(pos.x, pos.y - text.size),
                        Size::new(width, text.size),
                        click,
                    ) {
                        let (span, span_offset) = glyph.span;
                        let mut span_offset = span_offset as usize;
                        let Some(id) = span.id() else { continue };
                        let source = world.source(id).ok()?;
                        let node = source.find(span)?;
                        if matches!(node.kind(), SyntaxKind::Text | SyntaxKind::MathText)
                            && (click.x - pos.x) > width / 2.0
                        {
                            span_offset += glyph.range().len();
                        }

                        let span_offset = SourceSpanOffset {
                            span,
                            offset: span_offset,
                        };

                        return Some((span_offset, span_offset));
                    }

                    pos.x += width;
                }
            }

            FrameItem::Shape(shape, span) => {
                let Geometry::Rect(size) = shape.geometry else {
                    continue;
                };
                if is_in_rect(pos, size, click) {
                    let span = (*span).into();
                    return Some((span, span));
                }
            }

            FrameItem::Image(_, size, span) if is_in_rect(pos, *size, click) => {
                let span = (*span).into();
                return Some((span, span));
            }

            _ => {}
        }
    }

    None
}

/// Finds the output location in the document for a cursor position.
pub fn jump_from_cursor(document: &TypstDocument, source: &Source, cursor: usize) -> Vec<Position> {
    jump_from_cursor_(document, source, cursor).unwrap_or_default()
}

/// Finds the output location in the document for a cursor position.
fn jump_from_cursor_(
    document: &TypstDocument,
    source: &Source,
    cursor: usize,
) -> Option<Vec<Position>> {
    match document {
        TypstDocument::Paged(paged_doc) => {
            let node = LinkedNode::new(source.root())
                .leaf_at_compat(cursor)
                .filter(|node| !matches!(node.kind(), SyntaxKind::Text | SyntaxKind::MathText))?;

            let span = node.span();
            let mut positions = vec![];
            for (idx, page) in paged_doc.pages.iter().enumerate() {
                let mut min_dis = u64::MAX;
                let mut point = Point::default();
                if let Some(point) = find_in_frame(&page.frame, span, &mut min_dis, &mut point) {
                    if let Some(page) = NonZeroUsize::new(idx + 1) {
                        positions.push(Position { page, point });
                    }
                }
            }

            Some(positions)
        }
        _ => None,
    }
}

/// Finds the position of a span in a frame.
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
                    let dis = glyph
                        .span
                        .0
                        .into_raw()
                        .get()
                        .abs_diff(span.into_raw().get());
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

/// Whether a rectangle with the given size at the given position contains the
/// click position.
fn is_in_rect(pos: Point, size: Size, click: Point) -> bool {
    pos.x <= click.x && pos.x + size.x >= click.x && pos.y <= click.y && pos.y + size.y >= click.y
}
