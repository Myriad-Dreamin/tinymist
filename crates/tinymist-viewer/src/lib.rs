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
}

impl PageAccessibility {
    /// Creates a page accessibility tree from AccessKit nodes in paint order.
    pub fn new(nodes: impl IntoIterator<Item = Node>) -> Self {
        Self {
            nodes: nodes.into_iter().collect(),
        }
    }

    /// Returns the page AccessKit nodes in paint order.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Returns whether this page has no synthetic AccessKit nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
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

    /// Adds synthetic nodes to the active AccessKit update and attaches them to
    /// the owner widget node.
    pub fn push_accesskit_nodes(
        &self,
        tree_update: &mut TreeUpdate,
        owner: &mut Node,
        mut next_node_id: impl FnMut() -> NodeId,
        scale: f64,
    ) {
        let mut child_ids = Vec::with_capacity(self.nodes.len());
        for page_node in self.nodes.iter() {
            let node_id = next_node_id();
            child_ids.push(node_id);
            tree_update
                .nodes
                .push((node_id, scaled_access_node(page_node, scale)));
        }
        owner.set_children(child_ids);
    }

    pub(crate) fn push_node(&mut self, node: Node) {
        self.nodes.push(node);
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

fn scaled_access_node(node: &Node, scale: f64) -> Node {
    let mut node = node.clone();
    if scale != 1.0
        && let Some(bounds) = node.bounds()
    {
        node.set_bounds(transform_access_rect_bbox(Affine::scale(scale), bounds));
    }
    node
}

fn access_rect_contains(rect: AccessRect, point: Point) -> bool {
    point.x >= rect.x0 && point.x < rect.x1 && point.y >= rect.y0 && point.y < rect.y1
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
}
