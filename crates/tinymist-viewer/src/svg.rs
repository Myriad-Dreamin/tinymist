//! Draws SVG images as vector scenes.
//!
//! Adapted from the rendering code of `vello_svg` (Apache-2.0 OR MIT, the Vello Authors),
//! with three fixes from linebender/vello_svg#91: plain groups get no layer (a layer clipped
//! to a group's bounding box cuts off the outer half of strokes on its edge), clip paths use
//! all their shapes with their transforms, and flattened text is not transformed twice.
//! Transforms are accumulated from each group's own transform (see [`render_group`]).
//!
//! Only what can be drawn faithfully is supported. [`supports`] reports SVGs that use
//! anything else (patterns, masks, filters, embedded raster images); the caller draws those
//! with resvg instead.

use resvg::usvg;
use vello::Scene;
use vello::kurbo::{Affine, BezPath, Cap, Join, Point, Rect, Shape, Stroke};
use vello::peniko::color::DynamicColor;
use vello::peniko::{self, BlendMode, Brush, Color, Fill, Mix};

/// Whether every part of `tree` can be drawn by [`render_tree`].
pub(crate) fn supports(tree: &usvg::Tree) -> bool {
    group_supported(tree.root())
}

fn group_supported(group: &usvg::Group) -> bool {
    if group.mask().is_some() || !group.filters().is_empty() {
        return false;
    }
    group.children().iter().all(|node| match node {
        usvg::Node::Group(g) => group_supported(g),
        usvg::Node::Path(path) => {
            let paint_supported = |paint: &usvg::Paint| !matches!(paint, usvg::Paint::Pattern(_));
            path.fill().is_none_or(|f| paint_supported(f.paint()))
                && path.stroke().is_none_or(|s| paint_supported(s.paint()))
        }
        usvg::Node::Image(image) => match image.kind() {
            usvg::ImageKind::SVG(svg) => supports(svg),
            _ => false,
        },
        usvg::Node::Text(text) => group_supported(text.flattened()),
    }) && group
        .clip_path()
        .is_none_or(|clip| clip.clip_path().is_none() && clip_supported(clip.root()))
}

/// Clip paths are drawn as one non-zero path, so nested clip paths and even-odd clip rules
/// are not supported.
fn clip_supported(group: &usvg::Group) -> bool {
    group.clip_path().is_none()
        && group.children().iter().all(|node| match node {
            usvg::Node::Group(g) => clip_supported(g),
            usvg::Node::Path(path) => path
                .fill()
                .is_none_or(|f| f.rule() == usvg::FillRule::NonZero),
            usvg::Node::Text(_) => true,
            usvg::Node::Image(_) => false,
        })
}

/// Draws `tree` into a new scene, in the tree's own coordinates.
pub(crate) fn render_tree(tree: &usvg::Tree) -> Scene {
    let mut scene = Scene::new();
    render_group(&mut scene, tree.root(), Affine::IDENTITY);
    scene
}

/// Draws the children of `group`, whose own transform is already part of `transform`.
///
/// Transforms are built up from each group's own `transform()` rather than taken from
/// `abs_transform()`: usvg 0.47 leaves the transform of a `<use>` element out of the
/// absolute transforms below it, which turns Typst's PDF images (glyphs drawn with
/// `<use transform="matrix(1 0 0 -1 ..)">`) upside down.
fn render_group(scene: &mut Scene, group: &usvg::Group, transform: Affine) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => render_child_group(scene, g, transform),
            usvg::Node::Path(path) => render_path(scene, path, transform),
            usvg::Node::Image(image) => {
                if let usvg::ImageKind::SVG(svg) = image.kind()
                    && image.is_visible()
                {
                    render_group(scene, svg.root(), transform);
                }
            }
            usvg::Node::Text(text) => render_child_group(scene, text.flattened(), transform),
        }
    }
}

fn render_child_group(scene: &mut Scene, g: &usvg::Group, parent_transform: Affine) {
    let transform = parent_transform * to_affine(&g.transform());
    let blend = blend_mode(g.blend_mode());
    let alpha = g.opacity().get();

    let pushed = if let Some(clip_path) = g.clip_path() {
        // The clip path's shapes, in the group's coordinates. usvg puts a shape that has a
        // transform into a group of its own.
        let mut clip = BezPath::new();
        collect_clip(
            clip_path.root(),
            to_affine(&clip_path.transform()),
            &mut clip,
        );
        if clip.elements().is_empty() {
            // An empty clip path hides the whole group.
            return;
        }
        if clips_nothing(&clip, g) {
            // CAD exports often put a page-sized rectangle clip around every element.
            // Thousands of large layers can exhaust the renderer's memory, so leave them out.
            push_bounding_box_layer_if_needed(scene, g, blend, alpha, transform)
        } else {
            scene.push_layer(Fill::NonZero, blend, alpha, transform, &clip);
            true
        }
    } else {
        push_bounding_box_layer_if_needed(scene, g, blend, alpha, transform)
    };

    render_group(scene, g, transform);

    if pushed {
        scene.pop_layer();
    }
}

/// Pushes a layer for opacity, blending or isolation, clipped to the group's bounding box.
/// A plain group needs no layer: clipping it to its bounding box would cut off the outer half
/// of strokes lying on the edge of that box.
fn push_bounding_box_layer_if_needed(
    scene: &mut Scene,
    g: &usvg::Group,
    blend: BlendMode,
    alpha: f32,
    transform: Affine,
) -> bool {
    let needs_layer = alpha != 1.0 || g.blend_mode() != usvg::BlendMode::Normal || g.isolate();
    if needs_layer {
        let bbox = g.layer_bounding_box();
        let rect = Rect::from_origin_size(
            (bbox.x(), bbox.y()),
            (f64::from(bbox.width()), f64::from(bbox.height())),
        );
        scene.push_layer(Fill::NonZero, blend, alpha, transform, &rect);
    }
    needs_layer
}

/// Appends the shapes of a clip path to `out`, transformed by `transform`.
fn collect_clip(group: &usvg::Group, transform: Affine, out: &mut BezPath) {
    for node in group.children() {
        match node {
            usvg::Node::Path(path) if path.is_visible() => {
                let mut shape = to_bez_path(path);
                shape.apply_affine(transform);
                out.extend(shape);
            }
            usvg::Node::Group(g) => collect_clip(g, transform * to_affine(&g.transform()), out),
            usvg::Node::Text(text) => {
                let flattened = text.flattened();
                collect_clip(
                    flattened,
                    transform * to_affine(&flattened.transform()),
                    out,
                );
            }
            _ => {}
        }
    }
}

/// Whether `clip` is a rectangle that contains everything the group draws, so that clipping
/// changes nothing. Both are in the group's own coordinates.
fn clips_nothing(clip: &BezPath, group: &usvg::Group) -> bool {
    let bounds = clip.bounding_box();
    let is_rectangle = (clip.area().abs() - bounds.area()).abs() <= 1e-6 * bounds.area().max(1.0);
    let content = group.stroke_bounding_box();
    is_rectangle
        && bounds.x0 <= f64::from(content.left())
        && bounds.y0 <= f64::from(content.top())
        && bounds.x1 >= f64::from(content.right())
        && bounds.y1 >= f64::from(content.bottom())
}

fn render_path(scene: &mut Scene, path: &usvg::Path, transform: Affine) {
    if !path.is_visible() {
        return;
    }
    let shape = to_bez_path(path);
    let fill = |scene: &mut Scene| {
        if let Some(fill) = path.fill()
            && let Some((brush, brush_transform)) = to_brush(fill.paint(), fill.opacity())
        {
            let rule = match fill.rule() {
                usvg::FillRule::NonZero => Fill::NonZero,
                usvg::FillRule::EvenOdd => Fill::EvenOdd,
            };
            scene.fill(rule, transform, &brush, Some(brush_transform), &shape);
        }
    };
    let stroke = |scene: &mut Scene| {
        if let Some(stroke) = path.stroke()
            && let Some((brush, brush_transform)) = to_brush(stroke.paint(), stroke.opacity())
        {
            scene.stroke(
                &to_stroke(stroke),
                transform,
                &brush,
                Some(brush_transform),
                &shape,
            );
        }
    };
    match path.paint_order() {
        usvg::PaintOrder::FillAndStroke => {
            fill(scene);
            stroke(scene);
        }
        usvg::PaintOrder::StrokeAndFill => {
            stroke(scene);
            fill(scene);
        }
    }
}

fn blend_mode(mode: usvg::BlendMode) -> BlendMode {
    match mode {
        usvg::BlendMode::Normal => Mix::Normal,
        usvg::BlendMode::Multiply => Mix::Multiply,
        usvg::BlendMode::Screen => Mix::Screen,
        usvg::BlendMode::Overlay => Mix::Overlay,
        usvg::BlendMode::Darken => Mix::Darken,
        usvg::BlendMode::Lighten => Mix::Lighten,
        usvg::BlendMode::ColorDodge => Mix::ColorDodge,
        usvg::BlendMode::ColorBurn => Mix::ColorBurn,
        usvg::BlendMode::HardLight => Mix::HardLight,
        usvg::BlendMode::SoftLight => Mix::SoftLight,
        usvg::BlendMode::Difference => Mix::Difference,
        usvg::BlendMode::Exclusion => Mix::Exclusion,
        usvg::BlendMode::Hue => Mix::Hue,
        usvg::BlendMode::Saturation => Mix::Saturation,
        usvg::BlendMode::Color => Mix::Color,
        usvg::BlendMode::Luminosity => Mix::Luminosity,
    }
    .into()
}

fn to_affine(ts: &usvg::Transform) -> Affine {
    let usvg::Transform {
        sx,
        kx,
        ky,
        sy,
        tx,
        ty,
    } = ts;
    Affine::new([sx, ky, kx, sy, tx, ty].map(|&x| f64::from(x)))
}

fn to_stroke(stroke: &usvg::Stroke) -> Stroke {
    let mut out = Stroke::new(f64::from(stroke.width().get()))
        .with_caps(match stroke.linecap() {
            usvg::LineCap::Butt => Cap::Butt,
            usvg::LineCap::Round => Cap::Round,
            usvg::LineCap::Square => Cap::Square,
        })
        .with_join(match stroke.linejoin() {
            usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => Join::Miter,
            usvg::LineJoin::Round => Join::Round,
            usvg::LineJoin::Bevel => Join::Bevel,
        })
        .with_miter_limit(f64::from(stroke.miterlimit().get()));
    if let Some(dashes) = stroke.dasharray() {
        out = out.with_dashes(
            f64::from(stroke.dashoffset()),
            dashes.iter().map(|&d| f64::from(d)),
        );
    }
    out
}

fn to_bez_path(path: &usvg::Path) -> BezPath {
    use usvg::tiny_skia_path::PathSegment;

    let mut out = BezPath::new();
    // SVG starts a new subpath at the previous start point after a close; BezPath doesn't.
    let mut just_closed = false;
    let mut start = Point::ZERO;
    let point = |p: usvg::tiny_skia_path::Point| Point::new(f64::from(p.x), f64::from(p.y));
    for segment in path.data().segments() {
        if std::mem::take(&mut just_closed) && !matches!(segment, PathSegment::MoveTo(_)) {
            out.move_to(start);
        }
        match segment {
            PathSegment::MoveTo(p) => {
                start = point(p);
                out.move_to(start);
            }
            PathSegment::LineTo(p) => out.line_to(point(p)),
            PathSegment::QuadTo(p1, p2) => out.quad_to(point(p1), point(p2)),
            PathSegment::CubicTo(p1, p2, p3) => out.curve_to(point(p1), point(p2), point(p3)),
            PathSegment::Close => {
                just_closed = true;
                out.close_path();
            }
        }
    }
    out
}

fn to_brush(paint: &usvg::Paint, opacity: usvg::Opacity) -> Option<(Brush, Affine)> {
    let stops = |stops: &[usvg::Stop]| -> Vec<peniko::ColorStop> {
        stops
            .iter()
            .map(|stop| peniko::ColorStop {
                offset: stop.offset().get(),
                color: DynamicColor::from_alpha_color(Color::from_rgba8(
                    stop.color().red,
                    stop.color().green,
                    stop.color().blue,
                    (stop.opacity() * opacity).to_u8(),
                )),
            })
            .collect()
    };
    let extend = |spread: usvg::SpreadMethod| match spread {
        usvg::SpreadMethod::Pad => peniko::Extend::Pad,
        usvg::SpreadMethod::Reflect => peniko::Extend::Reflect,
        usvg::SpreadMethod::Repeat => peniko::Extend::Repeat,
    };
    match paint {
        usvg::Paint::Color(color) => Some((
            Brush::Solid(Color::from_rgba8(
                color.red,
                color.green,
                color.blue,
                opacity.to_u8(),
            )),
            Affine::IDENTITY,
        )),
        usvg::Paint::LinearGradient(gr) => {
            let gradient = peniko::Gradient::new_linear(
                Point::new(f64::from(gr.x1()), f64::from(gr.y1())),
                Point::new(f64::from(gr.x2()), f64::from(gr.y2())),
            )
            .with_stops(stops(gr.stops()).as_slice())
            .with_extend(extend(gr.spread_method()));
            Some((Brush::Gradient(gradient), to_affine(&gr.transform())))
        }
        usvg::Paint::RadialGradient(gr) => {
            let gradient = peniko::Gradient::new_two_point_radial(
                Point::new(f64::from(gr.fx()), f64::from(gr.fy())),
                0.0,
                Point::new(f64::from(gr.cx()), f64::from(gr.cy())),
                gr.r().get(),
            )
            .with_stops(stops(gr.stops()).as_slice())
            .with_extend(extend(gr.spread_method()));
            Some((Brush::Gradient(gradient), to_affine(&gr.transform())))
        }
        usvg::Paint::Pattern(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use masonry::core::Widget;
    use masonry::theme::default_property_set;
    use masonry::vello::kurbo::Size;
    use masonry_testing::{TestHarness, TestHarnessParams};

    use super::*;
    use crate::doc::PageCanvas;

    fn tree(svg: &str) -> usvg::Tree {
        usvg::Tree::from_str(svg, &usvg::Options::default()).unwrap()
    }

    /// Renders an SVG of 32 x 32 user units onto a white 32 x 32 pixel canvas.
    fn render(svg: &str) -> image::RgbaImage {
        let page = PageCanvas::new(Arc::new(render_tree(&tree(svg))), 1.0, Some(Color::WHITE));
        let mut params = TestHarnessParams::default();
        params.window_size = Size::new(32.0, 32.0);
        let mut harness =
            TestHarness::create_with(default_property_set(), page.with_auto_id(), params);
        harness.render()
    }

    fn svg(body: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="32" height="32" viewBox="0 0 32 32">{body}</svg>"#
        )
    }

    fn is_dark(image: &image::RgbaImage, x: u32, y: u32) -> bool {
        image.get_pixel(x, y).0[0] < 64
    }

    /// Sum of coverage in a pixel column.
    fn ink(image: &image::RgbaImage, x: u32) -> f64 {
        (0..image.height())
            .map(|y| f64::from(255 - image.get_pixel(x, y).0[0]) / 255.0)
            .sum()
    }

    #[test]
    fn flipped_use_is_drawn_in_place() {
        // Typst's PDF images draw glyphs like this. usvg 0.47 leaves the `<use>` transform
        // out of the absolute transforms, which turned them upside down.
        let image = render(&svg(
            r##"<defs><rect id="r" x="4" y="2" width="24" height="6"/></defs>
                <use xlink:href="#r" transform="matrix(1 0 0 -1 0 32)"/>"##,
        ));
        assert!(is_dark(&image, 16, 27));
        assert!(!is_dark(&image, 16, 5));
    }

    #[test]
    fn transformed_clip_shape_clips() {
        let image = render(&svg(
            r#"<clipPath id="c"><rect width="8" height="8" transform="scale(2)"/></clipPath>
                <g clip-path="url(#c)"><rect width="32" height="32"/></g>"#,
        ));
        assert!(is_dark(&image, 8, 8));
        assert!(!is_dark(&image, 24, 24));
    }

    #[test]
    fn empty_clip_path_hides_the_group() {
        let image = render(&svg(
            r#"<clipPath id="c"/><g clip-path="url(#c)"><rect width="32" height="32"/></g>"#,
        ));
        assert!(!is_dark(&image, 16, 16));
    }

    #[test]
    fn stroke_in_a_transformed_group_keeps_its_width() {
        // usvg puts the transformed path into a group. Clipping that group to its bounding
        // box used to cut the stroke, so it came out lighter than the same stroke drawn
        // without a group.
        let grouped = render(&svg(
            r#"<path d="M 2 8 H 14" stroke="black" stroke-width="0.4" transform="scale(2)"/>"#,
        ));
        let plain = render(&svg(
            r#"<path d="M 4 16 H 28" stroke="black" stroke-width="0.8"/>"#,
        ));
        assert!((ink(&grouped, 16) - ink(&plain, 16)).abs() < 0.05);
        assert!(ink(&plain, 16) > 0.7);
    }

    #[test]
    fn unsupported_features_are_reported() {
        assert!(supports(&tree(&svg(r#"<rect width="8" height="8"/>"#))));
        let unsupported = [
            r#"<pattern id="p" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="2" height="2"/></pattern><rect width="8" height="8" fill="url(#p)"/>"#,
            r#"<mask id="m"><rect width="4" height="4" fill="white"/></mask><rect width="8" height="8" mask="url(#m)"/>"#,
            r#"<filter id="f"><feGaussianBlur stdDeviation="1"/></filter><rect width="8" height="8" filter="url(#f)"/>"#,
            r#"<clipPath id="c"><rect width="4" height="4" clip-rule="evenodd"/></clipPath><rect width="8" height="8" clip-path="url(#c)"/>"#,
        ];
        for body in unsupported {
            assert!(!supports(&tree(&svg(body))), "{body}");
        }
    }
}
