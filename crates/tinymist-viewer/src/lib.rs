//! # Typst Vello
//!
//! A Vello scene drawer for Typst's frames.
//!
//! Some code are borrowed from [velyst](https://github.com/voxell-tech/velyst).

use std::fmt;

use ecow::EcoVec;
use reflexo::hash::Fingerprint;
use std::sync::Arc;
use vello::kurbo::{self, Affine, Point, Rect};
use vello::peniko;

pub mod doc;
pub mod incr;
pub mod protocol;
mod render;

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
    semantics: PageSemantics,
    content_hash: Fingerprint,
}

/// Semantic metadata for a rendered page.
#[derive(Debug, Clone, Default)]
pub struct PageSemantics {
    links: EcoVec<SemanticLink>,
}

impl PageSemantics {
    /// Returns all semantic links in paint order.
    pub fn links(&self) -> &[SemanticLink] {
        &self.links
    }

    /// Returns the topmost link at the provided page-coordinate point.
    pub fn hit_test_link(&self, point: Point) -> Option<&SemanticLink> {
        self.links
            .iter()
            .rev()
            .find(|link| link.rect.contains(point))
    }

    fn push_link(&mut self, link: SemanticLink) {
        self.links.push(link);
    }
}

/// A semantic link hit area in page coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticLink {
    /// The link destination.
    pub href: String,
    /// The clickable page-coordinate rectangle.
    pub rect: Rect,
}

impl SemanticLink {
    /// Creates a semantic link.
    pub fn new(href: impl Into<String>, rect: Rect) -> Self {
        Self {
            href: href.into(),
            rect,
        }
    }

    fn transformed(&self, transform: Affine) -> Self {
        Self {
            href: self.href.clone(),
            rect: transform_rect_bbox(transform, self.rect),
        }
    }
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

    /// Collects semantic metadata in page coordinates.
    pub fn page_semantics(&self) -> PageSemantics {
        let mut semantics = PageSemantics::default();
        self.collect_semantics(Affine::IDENTITY, &mut semantics);
        semantics
    }

    fn collect_semantics(&self, transform: Affine, semantics: &mut PageSemantics) {
        match self {
            VecScene::Group(group) => group.collect_semantics(transform, semantics),
            VecScene::Path(..) | VecScene::Scene(..) => {}
        }
    }
}

/// A group of scenes that are rendered together.
#[derive(Debug, Clone)]
pub struct GroupScene {
    clip: Option<kurbo::BezPath>,
    ts: Affine,
    scenes: EcoVec<(kurbo::Vec2, Arc<VecScene>)>,
    semantic_links: EcoVec<SemanticLink>,
}

impl GroupScene {
    /// Renders the group to a [`vello::Scene`].
    pub fn render(&self, scene: &mut vello::Scene) {
        if let Some(clip) = &self.clip {
            scene.push_clip_layer(peniko::Fill::NonZero, self.ts, clip);
        }
        let ts = self.ts;
        for (pos, elem) in self.scenes.iter() {
            let ts = ts.pre_translate(*pos);
            let mut sub_scene = vello::Scene::new();
            elem.render(&mut sub_scene);
            scene.append(&sub_scene, Some(ts));
        }
        if self.clip.is_some() {
            scene.pop_layer();
        }
    }

    fn collect_semantics(&self, transform: Affine, semantics: &mut PageSemantics) {
        let transform = transform * self.ts;
        for link in self.semantic_links.iter() {
            semantics.push_link(link.transformed(transform));
        }

        for (pos, elem) in self.scenes.iter() {
            elem.collect_semantics(transform.pre_translate(*pos), semantics);
        }
    }
}

fn transform_rect_bbox(transform: Affine, rect: Rect) -> Rect {
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

    Rect::new(x0, y0, x1, y1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_semantics_hit_tests_topmost_link() {
        let mut semantics = PageSemantics::default();
        semantics.push_link(SemanticLink::new(
            "https://bottom.example",
            Rect::new(0.0, 0.0, 20.0, 20.0),
        ));
        semantics.push_link(SemanticLink::new(
            "https://top.example",
            Rect::new(10.0, 10.0, 30.0, 30.0),
        ));

        assert_eq!(
            semantics
                .hit_test_link(Point::new(12.0, 12.0))
                .map(|link| link.href.as_str()),
            Some("https://top.example")
        );
        assert_eq!(
            semantics
                .hit_test_link(Point::new(5.0, 5.0))
                .map(|link| link.href.as_str()),
            Some("https://bottom.example")
        );
        assert!(semantics.hit_test_link(Point::new(40.0, 40.0)).is_none());
    }

    #[test]
    fn page_semantics_collects_transformed_nested_links() {
        let mut links = EcoVec::new();
        links.push(SemanticLink::new(
            "https://example.com",
            Rect::new(0.0, 0.0, 5.0, 10.0),
        ));
        let child = Arc::new(VecScene::Group(GroupScene {
            clip: None,
            ts: Affine::scale(2.0),
            scenes: EcoVec::new(),
            semantic_links: links,
        }));

        let mut scenes = EcoVec::new();
        scenes.push((kurbo::Vec2::new(10.0, 20.0), child));
        let scene = VecScene::Group(GroupScene {
            clip: None,
            ts: Affine::IDENTITY,
            scenes,
            semantic_links: EcoVec::new(),
        });

        let semantics = scene.page_semantics();
        assert_eq!(semantics.links().len(), 1);
        let link = &semantics.links()[0];
        assert_eq!(link.href, "https://example.com");
        assert_eq!(link.rect, Rect::new(10.0, 20.0, 20.0, 40.0));
        assert!(semantics.hit_test_link(Point::new(15.0, 30.0)).is_some());
        assert!(semantics.hit_test_link(Point::new(25.0, 30.0)).is_none());
    }
}
