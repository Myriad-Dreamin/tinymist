//! Jumping from and to source and the rendered document.

use std::{num::NonZeroUsize, ops::Range};

use tinymist_project::LspWorld;
use tinymist_std::typst::{TypstDocument, TypstPagedDocument};
use tinymist_world::debug_loc::SourceSpanOffset;
use typst::{
    World,
    introspection::PagedPosition as Position,
    layout::{Frame, FrameItem, Point, Size},
    syntax::{LinkedNode, Side, Source, Span, SyntaxKind},
    visualize::Geometry,
};

/// Finds a span range from a clicked physical position in a rendered paged
/// document.
pub fn jump_from_click(
    world: &LspWorld,
    frame: &Frame,
    click: Point,
) -> Option<(SourceSpanOffset, SourceSpanOffset)> {
    // Try to find a link first.
    for (pos, item) in frame.items() {
        if let FrameItem::Link(_dest, size) = item
            && is_in_rect(*pos, *size, click)
        {
            // todo: url reaction
            return None;
        }
    }

    // If there's no link, search for a jump target.
    for &(mut pos, ref item) in frame.items().rev() {
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
    let TypstDocument::Paged(paged_doc) = document else {
        return None;
    };

    let root = LinkedNode::new(source.root());
    let interesting_spans = source_interest_spans(&root, cursor)?;
    RenderedTarget::select_from_document(paged_doc, &interesting_spans).positions()
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum SearchDirection {
    Prev,
    Next,
}

struct LeafSearch<'a> {
    next: Option<LinkedNode<'a>>,
    direction: SearchDirection,
    equation_range: Range<usize>,
}

impl<'a> LeafSearch<'a> {
    fn from_neighbor(anchor: Option<&LinkedNode<'a>>, direction: SearchDirection) -> Option<Self> {
        let anchor = anchor?;
        let equation_range = equation_ancestor(anchor)?.range();
        Some(Self {
            next: neighbor_leaf(anchor, direction),
            direction,
            equation_range,
        })
    }

    fn next(&mut self) -> Option<LinkedNode<'a>> {
        let leaf = self.next.take()?;
        if !is_in_range(&leaf, &self.equation_range) {
            return None;
        }

        self.next = neighbor_leaf(&leaf, self.direction);
        Some(leaf)
    }
}

struct InterestSpans {
    // Sorted by span for lookup. The usize keeps the original search priority.
    lookup: Vec<(Span, usize)>,
}

impl InterestSpans {
    fn new(mut unsure: Vec<Span>, confident: Option<Span>) -> Option<Self> {
        if let Some(confident) = confident {
            unsure.push(confident);
        }

        let mut lookup = unsure
            .into_iter()
            .enumerate()
            .map(|(priority, span)| (span, priority))
            .collect::<Vec<_>>();

        if lookup.is_empty() {
            return None;
        }

        lookup.sort_by_key(|(span, priority)| (span.into_raw(), *priority));
        lookup.dedup_by(|(span, _), (previous, _)| *span == *previous);

        Some(Self { lookup })
    }

    fn priority_of(&self, span: Span) -> Option<usize> {
        let idx = self
            .lookup
            .binary_search_by_key(&span.into_raw(), |(interest, _)| interest.into_raw())
            .ok()?;
        let (interest, priority) = self.lookup[idx];
        (interest == span).then_some(priority)
    }
}

fn source_interest_spans(root: &LinkedNode, cursor: usize) -> Option<InterestSpans> {
    let mut collector = InterestCollector::default();

    let before = root.leaf_at(cursor, Side::Before);
    let after = root.leaf_at(cursor, Side::After);
    collector.search_outward(cursor, before.as_ref(), after.as_ref());

    collector.finish()
}

#[derive(Default)]
struct InterestCollector {
    unsure: Vec<Span>,
    confident: Option<Span>,
}

impl InterestCollector {
    fn visit(&mut self, node: &LinkedNode) {
        match source_target_class(node) {
            SourceTargetClass::Confident(span) => self.confident = Some(span),
            SourceTargetClass::Unsure(span) => self.unsure.push(span),
            SourceTargetClass::Impossible => {}
        }
    }

    fn search_outward<'a>(
        &mut self,
        cursor: usize,
        before: Option<&LinkedNode<'a>>,
        after: Option<&LinkedNode<'a>>,
    ) {
        // case: cursor is in the middle of a leaf, so both sides are the same leaf
        if same_leaf(before, after) {
            let Some(anchor) = before else {
                return;
            };
            self.visit(anchor);
            if self.is_done() {
                return;
            }

            let range = anchor.range();
            let midpoint = range.start + (range.len() / 2);
            let (first_direction, second_direction) = if cursor <= midpoint {
                (SearchDirection::Prev, SearchDirection::Next)
            } else {
                (SearchDirection::Next, SearchDirection::Prev)
            };

            let mut first_search = LeafSearch::from_neighbor(Some(anchor), first_direction);
            let mut second_search = LeafSearch::from_neighbor(Some(anchor), second_direction);
            self.search_alternating(&mut first_search, &mut second_search);
            return;
        }

        if let Some(before) = before {
            self.visit(before);
        }
        if self.is_done() {
            return;
        }

        if let Some(after) = after {
            self.visit(after);
        }
        if self.is_done() {
            return;
        }

        let mut prev_search = LeafSearch::from_neighbor(before.or(after), SearchDirection::Prev);
        let mut next_search = LeafSearch::from_neighbor(after.or(before), SearchDirection::Next);
        self.search_alternating(&mut prev_search, &mut next_search);
    }

    fn search_alternating(
        &mut self,
        first: &mut Option<LeafSearch<'_>>,
        second: &mut Option<LeafSearch<'_>>,
    ) {
        while !self.is_done() {
            let visited_first = self.visit_next(first);

            if self.is_done() {
                break;
            }

            let visited_second = self.visit_next(second);

            if !visited_first && !visited_second {
                break;
            }
        }
    }

    fn visit_next(&mut self, search: &mut Option<LeafSearch<'_>>) -> bool {
        if let Some(leaf) = search.as_mut().and_then(LeafSearch::next) {
            self.visit(&leaf);
            true
        } else {
            false
        }
    }

    fn is_done(&self) -> bool {
        self.confident.is_some()
    }

    fn finish(self) -> Option<InterestSpans> {
        InterestSpans::new(self.unsure, self.confident)
    }
}

enum SourceTargetClass {
    Confident(Span),
    Unsure(Span),
    Impossible,
}

fn source_target_class(node: &LinkedNode) -> SourceTargetClass {
    if node.kind() == SyntaxKind::Text {
        return SourceTargetClass::Confident(node.span());
    }

    let Some(equation) = equation_ancestor(node) else {
        return SourceTargetClass::Impossible;
    };

    if !is_in_range(node, &equation.range()) {
        return SourceTargetClass::Impossible;
    }

    if let Some(field_access) = math_field_access_ancestor(node) {
        return math_target_class(&field_access);
    }

    math_target_class(node)
}

fn math_target_class(node: &LinkedNode) -> SourceTargetClass {
    use SyntaxKind::*;

    match node.kind() {
        MathText | MathFieldAccess | MathShorthand | Escape | Plus | Minus | Star | Eq | EqEq
        | ExclEq | Lt | LtEq | Gt | GtEq | Dots | Arrow => {
            SourceTargetClass::Confident(node.span())
        }
        MathIdent | MathPrimes | Str | LeftParen | RightParen | LeftBrace | RightBrace => {
            SourceTargetClass::Unsure(node.span())
        }
        _ => SourceTargetClass::Impossible,
    }
}

fn equation_ancestor<'a>(node: &LinkedNode<'a>) -> Option<LinkedNode<'a>> {
    std::iter::successors(Some(node), |node| node.parent())
        .find(|node| node.kind() == SyntaxKind::Equation)
        .cloned()
}

fn math_field_access_ancestor<'a>(node: &LinkedNode<'a>) -> Option<LinkedNode<'a>> {
    std::iter::successors(Some(node), |node| node.parent())
        .take_while(|node| node.kind() != SyntaxKind::Equation)
        .filter(|node| node.kind() == SyntaxKind::MathFieldAccess)
        .last()
        .cloned()
}

fn is_in_range(node: &LinkedNode, range: &Range<usize>) -> bool {
    let node_range = node.range();
    range.start <= node_range.start && node_range.end <= range.end
}

fn same_leaf(left: Option<&LinkedNode>, right: Option<&LinkedNode>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left.span() == right.span() && left.range() == right.range(),
        (None, None) => true,
        _ => false,
    }
}

fn neighbor_leaf<'a>(node: &LinkedNode<'a>, direction: SearchDirection) -> Option<LinkedNode<'a>> {
    match direction {
        SearchDirection::Prev => node.prev_leaf(),
        SearchDirection::Next => node.next_leaf(),
    }
}

#[derive(Default)]
struct RenderedTarget {
    span: Option<Span>,
    positions: Vec<Position>,
    priority: Option<usize>,
}

impl RenderedTarget {
    fn select_from_document(
        document: &TypstPagedDocument,
        interesting_spans: &InterestSpans,
    ) -> Self {
        let mut target = Self::default();

        for (idx, page) in document.pages().iter().enumerate() {
            let Some(page_no) = NonZeroUsize::new(idx + 1) else {
                continue;
            };
            target.select_in_frame(&page.frame, page_no, Point::default(), interesting_spans);

            if target.priority == Some(0) {
                break;
            }
        }

        target
    }

    fn positions(self) -> Option<Vec<Position>> {
        self.span?;
        Some(self.positions)
    }

    fn select_in_frame(
        &mut self,
        frame: &Frame,
        page: NonZeroUsize,
        origin: Point,
        interesting_spans: &InterestSpans,
    ) {
        for &(mut pos, ref item) in frame.items() {
            if self.priority == Some(0) {
                break;
            }

            let item_origin = origin + pos;

            match item {
                FrameItem::Group(group) => {
                    // TODO: Handle transformation.
                    self.select_in_frame(&group.frame, page, item_origin, interesting_spans);
                }
                FrameItem::Text(text) => {
                    pos += origin;
                    for glyph in &text.glyphs {
                        let span = glyph.span.0;
                        if let Some(priority) = interesting_spans.priority_of(span) {
                            self.consider_hit(span, priority, Position { page, point: pos });
                            if self.priority == Some(0) {
                                break;
                            }
                        }
                        pos.x += glyph.x_advance.at(text.size);
                        pos.y += glyph.y_advance.at(text.size);
                    }
                }
                _ => {}
            }
        }
    }

    fn consider_hit(&mut self, span: Span, priority: usize, position: Position) {
        match self.priority {
            Some(target_priority) if priority > target_priority => {}
            Some(target_priority) if priority == target_priority => {
                if self.span != Some(span) {
                    return;
                }
                if self
                    .positions
                    .iter()
                    .all(|existing| existing.page != position.page)
                {
                    self.positions.push(position);
                }
            }
            _ => {
                self.span = Some(span);
                self.positions = vec![position];
                self.priority = Some(priority);
            }
        }
    }
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
    fn rendered_target_keeps_source_priority_order() {
        let source = Source::detached("$ a b $");
        let root = LinkedNode::new(source.root());
        let leaf_span = |text: &str| {
            let offset = source.text().find(text).unwrap();
            root.leaf_at(offset, Side::After).unwrap().span()
        };

        let confident = leaf_span("a");
        let unsure = leaf_span("b");
        let interesting_spans = InterestSpans::new(vec![unsure], Some(confident)).unwrap();
        assert_eq!(interesting_spans.priority_of(unsure), Some(0));
        assert_eq!(interesting_spans.priority_of(confident), Some(1));

        let page = NonZeroUsize::new(1).unwrap();
        let mut target = RenderedTarget::default();
        target.consider_hit(
            confident,
            interesting_spans.priority_of(confident).unwrap(),
            Position {
                page,
                point: Point::default(),
            },
        );
        target.consider_hit(
            unsure,
            interesting_spans.priority_of(unsure).unwrap(),
            Position {
                page,
                point: Point::default(),
            },
        );

        assert_eq!(target.span, Some(unsure));
        assert!(target.positions().is_some());
    }

    #[test]
    fn source_interest_spans_prefers_near_side_inside_unsure_leaf() {
        let source = Source::detached("$ a quad b $");
        let root = LinkedNode::new(source.root());
        let leaf_span = |text: &str| {
            let offset = source.text().find(text).unwrap();
            root.leaf_at(offset, Side::After).unwrap().span()
        };

        let a = leaf_span("a");
        let quad = leaf_span("quad");
        let b = leaf_span("b");
        let quad_start = source.text().find("quad").unwrap();

        let left = source_interest_spans(&root, quad_start + 1).unwrap();
        assert_eq!(left.priority_of(quad), Some(0));
        assert_eq!(left.priority_of(a), Some(1));
        assert_eq!(left.priority_of(b), None);

        let right = source_interest_spans(&root, quad_start + 3).unwrap();
        assert_eq!(right.priority_of(quad), Some(0));
        assert_eq!(right.priority_of(b), Some(1));
        assert_eq!(right.priority_of(a), None);
    }

    #[test]
    fn source_interest_spans_keeps_rendered_math_ident_before_later_arrow() {
        let source = Source::detached("$ HH -> K $");
        let root = LinkedNode::new(source.root());
        let leaf_span = |text: &str| {
            let offset = source.text().find(text).unwrap();
            root.leaf_at(offset, Side::After).unwrap().span()
        };

        let hh = leaf_span("HH");
        let arrow = leaf_span("->");
        let hh_start = source.text().find("HH").unwrap();

        let before_hh = source_interest_spans(&root, hh_start).unwrap();
        assert_eq!(before_hh.priority_of(hh), Some(0));
        assert_eq!(before_hh.priority_of(arrow), Some(1));

        let inside_hh = source_interest_spans(&root, hh_start + 1).unwrap();
        assert_eq!(inside_hh.priority_of(hh), Some(0));
        assert_eq!(inside_hh.priority_of(arrow), Some(1));
    }

    #[test]
    fn source_interest_spans_uses_outer_math_field_access() {
        let source = Source::detached("$ triangle.small.l $");
        let root = LinkedNode::new(source.root());
        let leaf = |text: &str| {
            let offset = source.text().find(text).unwrap();
            root.leaf_at(offset, Side::After).unwrap()
        };
        let outer = root
            .children()
            .find(|node| node.kind() == SyntaxKind::Equation)
            .unwrap()
            .children()
            .find(|node| node.kind() == SyntaxKind::Math)
            .unwrap()
            .children()
            .find(|node| node.kind() == SyntaxKind::MathFieldAccess)
            .unwrap();

        let triangle = leaf("triangle");
        let small = leaf("small");
        let l = leaf("l");

        assert_eq!(
            math_field_access_ancestor(&triangle).unwrap().span(),
            outer.span()
        );
        assert_eq!(
            math_field_access_ancestor(&small).unwrap().span(),
            outer.span()
        );
        assert_eq!(math_field_access_ancestor(&l).unwrap().span(), outer.span());

        let cursor = source.text().find("triangle").unwrap() + 1;
        let spans = source_interest_spans(&root, cursor).unwrap();
        assert_eq!(spans.priority_of(outer.span()), Some(0));
    }

    #[test]
    fn test() {
        snapshot_testing("jump_from_cursor", &|ctx, path| {
            let source = ctx.source_by_path(&path).unwrap();
            let document = ctx.success_doc().unwrap();

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

            with_settings!({
                description => format!("Jump cursor on {})", make_range_annotation(&source)),
            }, {
                assert_snapshot!(results);
            })
        });
    }
}
