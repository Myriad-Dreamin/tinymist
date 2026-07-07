//! # Typst Vello
//!
//! A Vello scene drawer for Typst's frames.
//!
//! Some code are borrowed from [velyst](https://github.com/voxell-tech/velyst).

use std::fmt;

use ecow::EcoVec;
use masonry::accesskit::{Action, Node, NodeId, Rect as AccessRect, Role, TreeUpdate};
use reflexo::hash::Fingerprint;
use std::sync::Arc;
use vello::kurbo::{self, Affine, Point};
use vello::peniko;

pub mod doc;
pub mod incr;
pub mod protocol;
mod render;
pub mod zoom_portal;

/// A raster image format resolved for an SVG-linked image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvgResourceFormat {
    /// JPEG image data.
    Jpeg,
    /// PNG image data.
    Png,
    /// GIF image data.
    Gif,
    /// WebP image data.
    Webp,
}

/// A raster image resolved from an SVG `<image href=...>` reference.
#[derive(Debug, Clone)]
pub struct SvgResource {
    /// The format of the encoded image data.
    pub format: SvgResourceFormat,
    /// The encoded image data.
    pub data: Arc<Vec<u8>>,
}

impl SvgResource {
    /// Creates a new resolved SVG image resource.
    pub fn new(format: SvgResourceFormat, data: impl Into<Vec<u8>>) -> Self {
        Self {
            format,
            data: Arc::new(data.into()),
        }
    }
}

/// Resolves image resources linked from inside SVG image data.
///
/// Vector documents only carry the encoded image bytes, so callers that still
/// know the original document or asset context can provide this hook to resolve
/// relative SVG image links.
pub trait SvgResourceResolver: Send + Sync {
    /// Resolves `href` from an SVG image.
    ///
    /// `svg_data` is the encoded SVG being parsed. Implementations can use it
    /// to recover a virtual base path for images that came from an asset store.
    fn resolve_svg_resource(&self, svg_data: &[u8], href: &str) -> Option<SvgResource>;
}

/// A vello scene corresponding to a typst page.
#[derive(Debug, Clone)]
pub struct VecPage {
    size: kurbo::Vec2,
    elem: Arc<VecScene>,
    accessibility: PageAccessibility,
    content_hash: Fingerprint,
}

/// AccessKit nodes for a rendered page.
#[derive(Debug, Clone, Default)]
pub struct PageAccessibility {
    nodes: EcoVec<Node>,
    text_runs: EcoVec<PageTextRun>,
}

impl PageAccessibility {
    /// Creates a page accessibility tree from AccessKit nodes in paint order.
    pub fn new(nodes: impl IntoIterator<Item = Node>) -> Self {
        Self {
            nodes: nodes.into_iter().collect(),
            text_runs: EcoVec::new(),
        }
    }

    /// Returns the page AccessKit nodes in paint order.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Returns selectable text runs in page coordinates.
    pub fn text_runs(&self) -> &[PageTextRun] {
        &self.text_runs
    }

    /// Returns whether this page has no synthetic AccessKit nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.text_runs.is_empty()
    }

    /// Returns the topmost link href at the provided page-coordinate point.
    pub fn hit_test_link(&self, point: Point) -> Option<&str> {
        self.nodes
            .iter()
            .rev()
            .find(|node| {
                node.role() == Role::Link
                    && node.supports_action(Action::Click)
                    && node
                        .bounds()
                        .is_some_and(|bounds| access_rect_contains(bounds, point))
            })
            .and_then(Node::value)
    }

    /// Returns whether the provided page-coordinate point is over selectable text.
    pub fn hit_test_text(&self, point: Point) -> Option<PageTextPosition> {
        self.text_runs
            .iter()
            .enumerate()
            .rev()
            .find_map(|(run_index, run)| {
                run.character_index_at_point(point)
                    .map(|character_index| PageTextPosition {
                        run_index,
                        character_index,
                    })
            })
    }

    /// Returns text covered by a selection.
    pub fn selected_text(&self, selection: PageTextSelection) -> String {
        let Some((start, end)) = selection.normalized() else {
            return String::new();
        };
        if start.run_index >= self.text_runs.len()
            || end.run_index >= self.text_runs.len()
            || start == end
        {
            return String::new();
        }

        let mut selected = String::new();
        for run_index in start.run_index..=end.run_index {
            let run = &self.text_runs[run_index];
            let start_index = if run_index == start.run_index {
                start.character_index
            } else {
                0
            };
            let end_index = if run_index == end.run_index {
                end.character_index
            } else {
                run.character_count()
            };
            selected.push_str(run.text_slice(start_index, end_index));
        }
        selected
    }

    /// Returns page-coordinate rectangles covered by a text selection.
    pub fn selection_rects(&self, selection: PageTextSelection) -> Vec<AccessRect> {
        let Some((start, end)) = selection.normalized() else {
            return Vec::new();
        };
        if start.run_index >= self.text_runs.len()
            || end.run_index >= self.text_runs.len()
            || start == end
        {
            return Vec::new();
        }

        let mut rects = Vec::new();
        for run_index in start.run_index..=end.run_index {
            let run = &self.text_runs[run_index];
            let start_index = if run_index == start.run_index {
                start.character_index
            } else {
                0
            };
            let end_index = if run_index == end.run_index {
                end.character_index
            } else {
                run.character_count()
            };
            if let Some(rect) = run.selection_rect(start_index, end_index) {
                rects.push(rect);
            }
        }
        rects
    }

    /// Adds synthetic nodes to the active AccessKit update and attaches them to
    /// the owner widget node.
    pub fn push_accesskit_nodes(
        &self,
        tree_update: &mut TreeUpdate,
        owner: &mut Node,
        mut next_node_id: impl FnMut() -> NodeId,
        scale: f64,
    ) {
        let mut child_ids = Vec::with_capacity(self.nodes.len() + self.text_runs.len());
        for page_node in self.nodes.iter() {
            let node_id = next_node_id();
            child_ids.push(node_id);
            tree_update
                .nodes
                .push((node_id, scaled_access_node(page_node, scale)));
        }
        for text_run in self.text_runs.iter() {
            let node_id = next_node_id();
            child_ids.push(node_id);
            tree_update
                .nodes
                .push((node_id, text_run.accesskit_node(scale)));
        }
        owner.set_children(child_ids);
    }

    pub(crate) fn push_node(&mut self, node: Node) {
        self.nodes.push(node);
    }

    pub(crate) fn push_text_run(&mut self, text_run: PageTextRun) {
        self.text_runs.push(text_run);
    }

    pub(crate) fn link_node(href: impl Into<String>, bounds: AccessRect) -> Node {
        let href = href.into();
        let mut node = Node::new(Role::Link);
        node.set_bounds(bounds);
        node.set_label(href.clone());
        node.set_value(href);
        node.add_action(Action::Click);
        node
    }
}

/// A selectable text run in page coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct PageTextRun {
    text: String,
    bounds: AccessRect,
    character_bounds: EcoVec<AccessRect>,
}

impl PageTextRun {
    /// Creates a page text run from text and per-character page-coordinate boxes.
    pub fn new(
        text: impl Into<String>,
        bounds: AccessRect,
        character_bounds: impl IntoIterator<Item = AccessRect>,
    ) -> Self {
        let text = text.into();
        let character_count = text.chars().count();
        let mut character_bounds: EcoVec<_> = character_bounds.into_iter().collect();
        if character_count == 0 {
            character_bounds.clear();
        } else if character_bounds.len() != character_count {
            character_bounds = split_access_rect_horizontally(bounds, character_count);
        }

        Self {
            text,
            bounds,
            character_bounds,
        }
    }

    /// Returns the plain text for this run.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the page-coordinate run bounds.
    pub fn bounds(&self) -> AccessRect {
        self.bounds
    }

    /// Returns page-coordinate per-character bounds.
    pub fn character_bounds(&self) -> &[AccessRect] {
        &self.character_bounds
    }

    /// Returns the number of selectable characters in this run.
    pub fn character_count(&self) -> usize {
        self.text.chars().count().min(self.character_bounds.len())
    }

    fn transformed(&self, transform: Affine) -> Self {
        Self {
            text: self.text.clone(),
            bounds: transform_access_rect_bbox(transform, self.bounds),
            character_bounds: self
                .character_bounds
                .iter()
                .map(|bounds| transform_access_rect_bbox(transform, *bounds))
                .collect(),
        }
    }

    fn accesskit_node(&self, scale: f64) -> Node {
        let mut node = Node::new(Role::TextRun);
        node.set_bounds(scale_access_rect(self.bounds, scale));
        node.set_value(self.text.clone());
        node.set_character_lengths(
            self.text
                .chars()
                .map(|c| c.len_utf8() as u8)
                .collect::<Vec<_>>(),
        );

        let character_count = self.character_count();
        let scaled_bounds = scale_access_rect(self.bounds, scale);
        let mut positions = Vec::with_capacity(character_count);
        let mut widths = Vec::with_capacity(character_count);
        for character_bounds in self.character_bounds.iter().take(character_count) {
            let character_bounds = scale_access_rect(*character_bounds, scale);
            positions.push((character_bounds.x0 - scaled_bounds.x0) as f32);
            widths.push((character_bounds.x1 - character_bounds.x0) as f32);
        }
        node.set_character_positions(positions);
        node.set_character_widths(widths);
        node
    }

    fn character_index_at_point(&self, point: Point) -> Option<usize> {
        if !access_rect_contains(self.bounds, point) {
            return None;
        }

        let character_count = self.character_count();
        if character_count == 0 {
            return Some(0);
        }

        for (index, bounds) in self
            .character_bounds
            .iter()
            .take(character_count)
            .enumerate()
        {
            if point.x >= bounds.x0 && point.x < bounds.x1 {
                let midpoint = (bounds.x0 + bounds.x1) / 2.0;
                return Some(if point.x < midpoint { index } else { index + 1 });
            }
        }

        if point.x < self.character_bounds[0].x0 {
            Some(0)
        } else {
            Some(character_count)
        }
    }

    fn selection_rect(&self, start: usize, end: usize) -> Option<AccessRect> {
        let character_count = self.character_count();
        let start = start.min(character_count);
        let end = end.min(character_count);
        if start >= end {
            return None;
        }

        self.character_bounds
            .iter()
            .skip(start)
            .take(end - start)
            .copied()
            .reduce(access_rect_union)
    }

    fn text_slice(&self, start: usize, end: usize) -> &str {
        let character_count = self.character_count();
        let start = start.min(character_count);
        let end = end.min(character_count);
        if start >= end {
            return "";
        }

        let start_byte = char_to_byte_index(&self.text, start);
        let end_byte = char_to_byte_index(&self.text, end);
        &self.text[start_byte..end_byte]
    }
}

/// A position inside the page text semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PageTextPosition {
    /// Index into [`PageAccessibility::text_runs`].
    pub run_index: usize,
    /// Character index inside the run.
    pub character_index: usize,
}

/// A text selection inside the page text semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageTextSelection {
    /// Fixed end of the selection.
    pub anchor: PageTextPosition,
    /// Active end of the selection.
    pub focus: PageTextPosition,
}

impl PageTextSelection {
    /// Creates a collapsed selection at a text position.
    pub fn collapsed(position: PageTextPosition) -> Self {
        Self {
            anchor: position,
            focus: position,
        }
    }

    /// Returns whether the selection is collapsed.
    pub fn is_collapsed(self) -> bool {
        self.anchor == self.focus
    }

    fn normalized(self) -> Option<(PageTextPosition, PageTextPosition)> {
        if self.is_collapsed() {
            None
        } else if self.anchor <= self.focus {
            Some((self.anchor, self.focus))
        } else {
            Some((self.focus, self.anchor))
        }
    }
}

fn scaled_access_node(node: &Node, scale: f64) -> Node {
    let mut node = node.clone();
    if scale != 1.0
        && let Some(bounds) = node.bounds()
    {
        node.set_bounds(scale_access_rect(bounds, scale));
    }
    node
}

fn scale_access_rect(rect: AccessRect, scale: f64) -> AccessRect {
    if scale == 1.0 {
        rect
    } else {
        transform_access_rect_bbox(Affine::scale(scale), rect)
    }
}

fn access_rect_contains(rect: AccessRect, point: Point) -> bool {
    point.x >= rect.x0 && point.x < rect.x1 && point.y >= rect.y0 && point.y < rect.y1
}

fn access_rect_union(a: AccessRect, b: AccessRect) -> AccessRect {
    AccessRect::new(
        a.x0.min(b.x0),
        a.y0.min(b.y0),
        a.x1.max(b.x1),
        a.y1.max(b.y1),
    )
}

fn split_access_rect_horizontally(rect: AccessRect, count: usize) -> EcoVec<AccessRect> {
    if count == 0 {
        return EcoVec::new();
    }

    let width = rect.width() / count as f64;
    (0..count)
        .map(|index| {
            let x0 = rect.x0 + width * index as f64;
            AccessRect::new(x0, rect.y0, x0 + width, rect.y1)
        })
        .collect()
}

fn char_to_byte_index(text: &str, character_index: usize) -> usize {
    text.char_indices()
        .nth(character_index)
        .map_or(text.len(), |(byte_index, _)| byte_index)
}

/// A scene that can be rendered to a [`vello::Scene`].
#[derive(Clone)]
pub enum VecScene {
    /// A group of scenes that are rendered together.
    Group(GroupScene),
    /// A path that is rendered to a scene.
    Path(kurbo::BezPath, peniko::Color),
    /// A scene that is rendered to a scene.
    Scene(Box<vello::Scene>, Option<kurbo::Affine>),
}

impl fmt::Debug for VecScene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VecScene::Group(..) => write!(f, "GroupScene"),
            VecScene::Path(..) => write!(f, "Path"),
            VecScene::Scene(..) => write!(f, "Scene"),
        }
    }
}

impl VecScene {
    /// Renders the scene to a [`vello::Scene`].
    pub fn render(&self, scene: &mut vello::Scene) {
        match self {
            VecScene::Group(group) => group.render(scene),
            VecScene::Path(path, color) => {
                scene.fill(
                    peniko::Fill::NonZero,
                    Affine::IDENTITY,
                    &peniko::Brush::Solid(*color),
                    None,
                    path,
                );
            }
            VecScene::Scene(sub, transform) => scene.append(sub, *transform),
        }
    }

    /// Collects AccessKit nodes in page coordinates.
    pub fn page_accessibility(&self) -> PageAccessibility {
        let mut accessibility = PageAccessibility::default();
        self.collect_accessibility(Affine::IDENTITY, &mut accessibility);
        accessibility
    }

    fn collect_accessibility(&self, transform: Affine, accessibility: &mut PageAccessibility) {
        match self {
            VecScene::Group(group) => group.collect_accessibility(transform, accessibility),
            VecScene::Path(..) | VecScene::Scene(..) => {}
        }
    }
}

/// A group of scenes that are rendered together.
#[derive(Debug, Clone)]
pub struct GroupScene {
    clip: Option<kurbo::BezPath>,
    ts: Affine,
    items: EcoVec<GroupSceneItem>,
}

/// An item inside a [`GroupScene`], ordered as emitted by the renderer.
#[derive(Debug, Clone)]
pub enum GroupSceneItem {
    /// A visual scene drawn at the given group-local position.
    Scene {
        /// The group-local position of the scene.
        pos: kurbo::Vec2,
        /// The scene to render.
        scene: Arc<VecScene>,
    },
    /// An AccessKit node associated with the group.
    Accessibility(Node),
    /// A selectable text run associated with the group.
    Text(PageTextRun),
}

impl GroupScene {
    /// Renders the group to a [`vello::Scene`].
    pub fn render(&self, scene: &mut vello::Scene) {
        if let Some(clip) = &self.clip {
            scene.push_clip_layer(peniko::Fill::NonZero, self.ts, clip);
        }
        let ts = self.ts;
        for item in self.items.iter() {
            if let GroupSceneItem::Scene {
                pos,
                scene: child_scene,
            } = item
            {
                let ts = ts.pre_translate(*pos);
                let mut sub_scene = vello::Scene::new();
                child_scene.render(&mut sub_scene);
                scene.append(&sub_scene, Some(ts));
            }
        }
        if self.clip.is_some() {
            scene.pop_layer();
        }
    }

    fn collect_accessibility(&self, transform: Affine, accessibility: &mut PageAccessibility) {
        let transform = transform * self.ts;
        for item in self.items.iter() {
            match item {
                GroupSceneItem::Scene {
                    pos,
                    scene: child_scene,
                } => {
                    child_scene.collect_accessibility(transform.pre_translate(*pos), accessibility);
                }
                GroupSceneItem::Accessibility(node) => {
                    let mut node = node.clone();
                    if let Some(bounds) = node.bounds() {
                        node.set_bounds(transform_access_rect_bbox(transform, bounds));
                    }
                    accessibility.push_node(node);
                }
                GroupSceneItem::Text(text_run) => {
                    accessibility.push_text_run(text_run.transformed(transform));
                }
            }
        }
    }
}

fn transform_access_rect_bbox(transform: Affine, rect: AccessRect) -> AccessRect {
    let points = [
        Point::new(rect.x0, rect.y0),
        Point::new(rect.x1, rect.y0),
        Point::new(rect.x1, rect.y1),
        Point::new(rect.x0, rect.y1),
    ]
    .map(|point| transform * point);

    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for point in points {
        x0 = x0.min(point.x);
        y0 = y0.min(point.y);
        x1 = x1.max(point.x);
        y1 = y1.max(point.y);
    }

    AccessRect::new(x0, y0, x1, y1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access_rect(x0: f64, y0: f64, x1: f64, y1: f64) -> AccessRect {
        AccessRect::new(x0, y0, x1, y1)
    }

    #[test]
    fn page_accessibility_hit_tests_topmost_link_node() {
        let mut accessibility = PageAccessibility::default();
        accessibility.push_node(PageAccessibility::link_node(
            "https://bottom.example",
            access_rect(0.0, 0.0, 20.0, 20.0),
        ));
        accessibility.push_node(PageAccessibility::link_node(
            "https://top.example",
            access_rect(10.0, 10.0, 30.0, 30.0),
        ));

        assert_eq!(
            accessibility.hit_test_link(Point::new(12.0, 12.0)),
            Some("https://top.example")
        );
        assert_eq!(
            accessibility.hit_test_link(Point::new(5.0, 5.0)),
            Some("https://bottom.example")
        );
        assert!(
            accessibility
                .hit_test_link(Point::new(40.0, 40.0))
                .is_none()
        );
    }

    #[test]
    fn page_accessibility_collects_transformed_nested_link_nodes() {
        let mut child_items = EcoVec::new();
        child_items.push(GroupSceneItem::Accessibility(PageAccessibility::link_node(
            "https://example.com",
            access_rect(0.0, 0.0, 5.0, 10.0),
        )));
        let child = Arc::new(VecScene::Group(GroupScene {
            clip: None,
            ts: Affine::scale(2.0),
            items: child_items,
        }));

        let mut items = EcoVec::new();
        items.push(GroupSceneItem::Scene {
            pos: kurbo::Vec2::new(10.0, 20.0),
            scene: child,
        });
        let scene = VecScene::Group(GroupScene {
            clip: None,
            ts: Affine::IDENTITY,
            items,
        });

        let accessibility = scene.page_accessibility();
        let [link] = accessibility.nodes() else {
            panic!("expected one AccessKit link node");
        };
        assert_eq!(link.role(), Role::Link);
        assert_eq!(link.value(), Some("https://example.com"));
        assert_eq!(link.bounds(), Some(access_rect(10.0, 20.0, 20.0, 40.0)));
        assert!(link.supports_action(Action::Click));
        assert!(
            accessibility
                .hit_test_link(Point::new(15.0, 30.0))
                .is_some()
        );
        assert!(
            accessibility
                .hit_test_link(Point::new(25.0, 30.0))
                .is_none()
        );
    }

    #[test]
    fn page_accessibility_hit_tests_links_in_group_item_order() {
        let mut child_items = EcoVec::new();
        child_items.push(GroupSceneItem::Accessibility(PageAccessibility::link_node(
            "https://bottom.example",
            access_rect(0.0, 0.0, 20.0, 20.0),
        )));
        let child = Arc::new(VecScene::Group(GroupScene {
            clip: None,
            ts: Affine::IDENTITY,
            items: child_items,
        }));

        let mut items = EcoVec::new();
        items.push(GroupSceneItem::Scene {
            pos: kurbo::Vec2::ZERO,
            scene: child,
        });
        items.push(GroupSceneItem::Accessibility(PageAccessibility::link_node(
            "https://top.example",
            access_rect(0.0, 0.0, 20.0, 20.0),
        )));
        let scene = VecScene::Group(GroupScene {
            clip: None,
            ts: Affine::IDENTITY,
            items,
        });

        assert_eq!(
            scene
                .page_accessibility()
                .hit_test_link(Point::new(10.0, 10.0)),
            Some("https://top.example")
        );
    }

    #[test]
    fn page_accessibility_collects_transformed_text_runs() {
        let text_run = PageTextRun::new(
            "hi",
            access_rect(0.0, 0.0, 20.0, 10.0),
            [
                access_rect(0.0, 0.0, 10.0, 10.0),
                access_rect(10.0, 0.0, 20.0, 10.0),
            ],
        );

        let mut items = EcoVec::new();
        items.push(GroupSceneItem::Text(text_run));
        let scene = VecScene::Group(GroupScene {
            clip: None,
            ts: Affine::translate((5.0, 10.0)) * Affine::scale(2.0),
            items,
        });

        let accessibility = scene.page_accessibility();
        let [run] = accessibility.text_runs() else {
            panic!("expected one text run");
        };
        assert_eq!(run.text(), "hi");
        assert_eq!(run.bounds(), access_rect(5.0, 10.0, 45.0, 30.0));
        assert_eq!(
            run.character_bounds(),
            &[
                access_rect(5.0, 10.0, 25.0, 30.0),
                access_rect(25.0, 10.0, 45.0, 30.0),
            ]
        );
    }

    #[test]
    fn page_accessibility_selects_text_by_page_point() {
        let mut accessibility = PageAccessibility::default();
        accessibility.push_text_run(PageTextRun::new(
            "hello",
            access_rect(0.0, 0.0, 50.0, 10.0),
            (0..5).map(|index| {
                let x0 = index as f64 * 10.0;
                access_rect(x0, 0.0, x0 + 10.0, 10.0)
            }),
        ));

        let anchor = accessibility
            .hit_test_text(Point::new(1.0, 5.0))
            .expect("left edge should hit text");
        let focus = accessibility
            .hit_test_text(Point::new(49.0, 5.0))
            .expect("right edge should hit text");
        let selection = PageTextSelection { anchor, focus };

        assert_eq!(accessibility.selected_text(selection), "hello");
        assert_eq!(
            accessibility.selection_rects(selection),
            [access_rect(0.0, 0.0, 50.0, 10.0)]
        );
    }
}
