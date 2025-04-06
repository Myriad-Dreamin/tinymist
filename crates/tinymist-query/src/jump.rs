//! Jumping from and to source and the rendered document.

use std::num::NonZeroUsize;

use tinymist_project::LspWorld;
use tinymist_std::typst::TypstDocument;
use tinymist_world::debug_loc::SourceSpanOffset;
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
    // todo: leaf_at_compat only matches the text before the cursor, but we could
    // also match a text if it is after the cursor
    // The case `leaf_at_compat` will match: `Hello|`
    // FIXME: The case `leaf_at_compat` will not match: `|Hello`
    let node = LinkedNode::new(source.root()).leaf_at_compat(cursor)?;
    // todo: When we click on a label or some math operators, we seems likely also
    // be able to jump to some place.
    if !matches!(node.kind(), SyntaxKind::Text | SyntaxKind::MathText) {
        return None;
    };

    let span = node.span();
    let offset = cursor.saturating_sub(node.offset());

    // todo: The cursor may not exact hit at the start of some AST node. For
    // example, the cursor in the text element `Hell|o` is offset by 4 from the
    // node. It seems not pretty if we ignore the offset completely.
    let _ = offset;

    match document {
        TypstDocument::Paged(paged_doc) => {
            // We checks whether there are any elements exactly matching the
            // cursor position.
            let mut positions = vec![];

            // Unluckily, we might not be able to find the exact spans, so we
            // need to find the closest one at the same time.
            let mut min_page = 0;
            let mut min_point = Point::default();
            let mut min_dis = u64::MAX;

            for (idx, page) in paged_doc.pages.iter().enumerate() {
                // In a page, we try to find a closer span than the existing found one.
                let mut p_dis = min_dis;

                if let Some(point) = find_in_frame(&page.frame, span, &mut p_dis, &mut min_point) {
                    if let Some(page) = NonZeroUsize::new(idx + 1) {
                        positions.push(Position { page, point });
                    }
                }

                // In this page, we found a closer span and update.
                if p_dis != min_dis {
                    min_page = idx;
                    min_dis = p_dis;
                }
            }

            // If we didn't find any exact span, we add the closest one in the same page.
            if positions.is_empty() && min_dis != u64::MAX {
                positions.push(Position {
                    page: NonZeroUsize::new(min_page + 1)?,
                    point: min_point,
                });
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

                // We at least require that the span is in the same file.
                let is_same_file = glyph.span.0.id() == span.id();
                if is_same_file {
                    // The numbers are not offsets but a unique id on the AST tree which are
                    // nicely divided.
                    // FIXME: since typst v0.13.0, the numbers are not only the ids, but also raw
                    // ranges, See [`Span::range`].
                    let glyph_num = glyph.span.0.into_raw();
                    let span_num = span.into_raw().get();
                    let dis = glyph_num.get().abs_diff(span_num);
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

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use super::*;
    use crate::tests::*;

    #[test]
    fn test() {
        snapshot_testing("jump_from_cursor", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();
            let docs = find_module_level_docs(&source).unwrap_or_default();
            let properties = get_test_properties(&docs);

            let graph = compile_doc_for_test(ctx, &properties);
            let document = graph.snap.success_doc.as_ref().unwrap();

            let cursors = find_test_range_(&source);

            let results = cursors
                .map(|cursor| {
                    let points = jump_from_cursor(document, &source, cursor);

                    if points.is_empty() {
                        return "nothing".to_string();
                    }

                    points
                        .iter()
                        .map(|pos| {
                            let page = pos.page.get();
                            let point = pos.point;
                            format!("{page},{:.3}pt,{:.3}pt", point.x.to_pt(), point.y.to_pt())
                        })
                        .join(";")
                })
                .join("\n");

            insta::with_settings!({
                description => format!("Jump cursor on {})", make_range_annoation(&source)),
            }, {
                assert_snapshot!(results);
            })
        });
    }
}
