//! Draws SVG images as vector scenes.
//!
//! Adapted from the rendering code of `vello_svg` (Apache-2.0 OR MIT, the Vello Authors),
//! with the fixes from linebender/vello_svg#91: plain groups get no layer (a layer clipped
//! to a group's bounding box cuts off the outer half of strokes on its edge), clip paths use
//! all their shapes, united as in SVG, and flattened text is not transformed twice.
//! Transforms are accumulated from each group's own transform (see [`render_group`]).
//! Gradients work around three differences between Vello 0.7 and resvg (see
//! [`gradient_stops`] and [`conformal_linear_gradient`]).
//!
//! Only what can be drawn faithfully is supported. [`supports`] reports SVGs that use
//! anything else (patterns, masks, filters, embedded raster images, nested clip paths, the
//! colour-dodge blend mode, and round or square caps on zero-length dashes); the caller
//! draws those with resvg instead.

use resvg::usvg;
use vello::Scene;
use vello::kurbo::{Affine, BezPath, Cap, Join, Point, Rect, Shape, Stroke};
use vello::peniko::color::DynamicColor;
use vello::peniko::{self, BlendMode, Brush, Color, Compose, Fill, Mix};

/// Whether every part of `tree` can be drawn by [`render_tree`].
pub(crate) fn supports(tree: &usvg::Tree) -> bool {
    group_supported(tree.root())
}

fn group_supported(group: &usvg::Group) -> bool {
    // Vello 0.7's colour dodge leaves stray lines where the source is transparent.
    if group.mask().is_some()
        || !group.filters().is_empty()
        || group.blend_mode() == usvg::BlendMode::ColorDodge
    {
        return false;
    }
    group.children().iter().all(|node| match node {
        usvg::Node::Group(g) => group_supported(g),
        usvg::Node::Path(path) => {
            path.fill().is_none_or(|f| paint_supported(f.paint()))
                && path
                    .stroke()
                    .is_none_or(|s| paint_supported(s.paint()) && caps_supported(s))
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

/// Patterns aren't supported, and neither are radial gradients whose focal circle is as
/// large as the end circle, which resvg draws as a solid colour.
fn paint_supported(paint: &usvg::Paint) -> bool {
    match paint {
        usvg::Paint::Pattern(_) => false,
        usvg::Paint::RadialGradient(gr) => gr.fr().get() < gr.r().get(),
        _ => true,
    }
}

/// Round and square caps on zero-length dashes draw a dot or square in SVG, but nothing when
/// stroked by kurbo. Zero-length subpaths are drawn by [`zero_length_caps`].
fn caps_supported(stroke: &usvg::Stroke) -> bool {
    stroke.linecap() == usvg::LineCap::Butt
        || !stroke
            .dasharray()
            .is_some_and(|dashes| dashes.contains(&0.0))
}

/// The points of the subpaths of `data` that stay on their start point.
fn zero_length_subpaths(data: &usvg::tiny_skia_path::Path) -> Vec<Point> {
    use usvg::tiny_skia_path::PathVerb;

    let points = data.points();
    let mut out = Vec::new();
    let mut next = 0;
    let mut start = None;
    let mut moved = false;
    for verb in data.verbs() {
        let count = match verb {
            PathVerb::Move => {
                if let Some(p) = start
                    && !moved
                {
                    out.push(p);
                }
                start = points.get(next).copied();
                moved = false;
                next += 1;
                continue;
            }
            PathVerb::Line => 1,
            PathVerb::Quad => 2,
            PathVerb::Cubic => 3,
            PathVerb::Close => continue,
        };
        let end = (next + count).min(points.len());
        moved = moved || points[next..end].iter().any(|p| Some(*p) != start);
        next = end;
    }
    if let Some(p) = start
        && !moved
    {
        out.push(p);
    }
    out.into_iter()
        .map(|p| Point::new(f64::from(p.x), f64::from(p.y)))
        .collect()
}

/// The dots and squares that round and square caps draw on zero-length subpaths, which
/// kurbo's stroker leaves out. Squares are aligned with the path's own axes.
fn zero_length_caps(path: &usvg::Path, stroke: &usvg::Stroke) -> Option<BezPath> {
    if stroke.linecap() == usvg::LineCap::Butt {
        return None;
    }
    let points = zero_length_subpaths(path.data());
    if points.is_empty() {
        return None;
    }
    let half = f64::from(stroke.width().get()) / 2.0;
    let mut out = BezPath::new();
    for p in points {
        if stroke.linecap() == usvg::LineCap::Round {
            out.extend(vello::kurbo::Circle::new(p, half).path_elements(0.1));
        } else {
            out.extend(
                Rect::new(p.x - half, p.y - half, p.x + half, p.y + half).path_elements(0.1),
            );
        }
    }
    Some(out)
}

/// Nested clip paths and images inside clip paths are not supported.
fn clip_supported(group: &usvg::Group) -> bool {
    group.clip_path().is_none()
        && group.children().iter().all(|node| match node {
            usvg::Node::Group(g) => clip_supported(g),
            usvg::Node::Path(_) => true,
            usvg::Node::Text(text) => clip_supported(text.flattened()),
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

    let mut mask = None;
    let pushed = if let Some(clip_path) = g.clip_path() {
        // The clip path's shapes, in the group's coordinates. usvg puts a shape that has a
        // transform into a group of its own.
        let mut shapes = Vec::new();
        collect_clip(
            clip_path.root(),
            to_affine(&clip_path.transform()),
            &mut shapes,
        );
        match shapes.as_slice() {
            // An empty clip path hides the whole group.
            [] => return,
            [(clip, Fill::NonZero)] => {
                if clips_nothing(clip, g) {
                    // CAD exports often put a page-sized rectangle clip around every
                    // element. Thousands of large layers can exhaust the renderer's memory,
                    // so leave them out.
                    push_bounding_box_layer_if_needed(scene, g, blend, alpha, transform)
                } else {
                    scene.push_layer(Fill::NonZero, blend, alpha, transform, clip);
                    true
                }
            }
            _ if merging_unites(&shapes) => {
                let mut clip = BezPath::new();
                for (shape, _) in &shapes {
                    clip.extend(shape.elements().iter().copied());
                }
                scene.push_layer(Fill::NonZero, blend, alpha, transform, &clip);
                true
            }
            _ => {
                // SVG unites the shapes of a clip path, each with its own clip rule. One
                // non-zero path would leave holes where oppositely wound shapes overlap, so
                // the shapes are drawn into a mask after the content.
                let bounds = shapes
                    .iter()
                    .map(|(shape, _)| shape.bounding_box())
                    .reduce(|a, b| a.union(b))
                    .unwrap_or_default();
                scene.push_layer(Fill::NonZero, blend, alpha, transform, &bounds);
                mask = Some((shapes, bounds));
                true
            }
        }
    } else {
        push_bounding_box_layer_if_needed(scene, g, blend, alpha, transform)
    };

    render_group(scene, g, transform);

    if let Some((shapes, bounds)) = mask {
        // Keep the content only where a clip shape covers it.
        scene.push_layer(
            Fill::NonZero,
            BlendMode::new(Mix::Normal, Compose::DestIn),
            1.0,
            transform,
            &bounds,
        );
        for (shape, rule) in &shapes {
            scene.fill(*rule, transform, Color::BLACK, None, shape);
        }
        scene.pop_layer();
    }
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

/// Appends the shapes of a clip path and their clip rules to `out`, transformed by
/// `transform`.
fn collect_clip(group: &usvg::Group, transform: Affine, out: &mut Vec<(BezPath, Fill)>) {
    for node in group.children() {
        match node {
            usvg::Node::Path(path) if path.is_visible() => {
                let mut shape = to_bez_path(path);
                shape.apply_affine(transform);
                let rule = match path.fill().map(|fill| fill.rule()) {
                    Some(usvg::FillRule::EvenOdd) => Fill::EvenOdd,
                    _ => Fill::NonZero,
                };
                out.push((shape, rule));
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
    let Some(bounds) = axis_aligned_rect(clip) else {
        return false;
    };
    let content = group.stroke_bounding_box();
    bounds.x0 <= f64::from(content.left())
        && bounds.y0 <= f64::from(content.top())
        && bounds.x1 >= f64::from(content.right())
        && bounds.y1 >= f64::from(content.bottom())
}

/// The rectangle `path` outlines, if it is one closed outline of four straight, axis-aligned
/// sides.
fn axis_aligned_rect(path: &BezPath) -> Option<Rect> {
    use vello::kurbo::PathEl;

    let mut points = [Point::ZERO; 5];
    let mut count = 0;
    for (i, element) in path.elements().iter().enumerate() {
        let point = match (i, element) {
            (0, PathEl::MoveTo(p)) | (1.., PathEl::LineTo(p)) => *p,
            (1.., PathEl::ClosePath) => continue,
            _ => return None,
        };
        if count == points.len() {
            return None;
        }
        points[count] = point;
        count += 1;
    }
    if count == 5 && points[4] == points[0] {
        count = 4;
    }
    if count != 4 {
        return None;
    }
    // Sides alternate between horizontal and vertical, and none has zero length.
    let is_rectangle = (0..4).all(|i| {
        let (a, b, c) = (points[i], points[(i + 1) % 4], points[(i + 2) % 4]);
        (a.x == b.x || a.y == b.y) && (a.x == b.x) != (b.x == c.x) && a != b
    });
    is_rectangle.then(|| path.bounding_box())
}

/// Whether filling `shapes` as one non-zero path gives their union: every shape is non-zero
/// and every subpath is made of straight lines that all turn the same way (rectangles and
/// other convex polygons, all wound in one direction). Overlapping parts then add up instead
/// of cancelling out.
fn merging_unites(shapes: &[(BezPath, Fill)]) -> bool {
    use vello::kurbo::PathEl;

    let mut direction = 0.0_f64;
    for (shape, rule) in shapes {
        if *rule != Fill::NonZero {
            return false;
        }
        let mut points: Vec<Point> = Vec::new();
        let mut check = |points: &mut Vec<Point>| -> bool {
            if points.len() > 1 && points.first() == points.last() {
                points.pop();
            }
            let n = points.len();
            for i in 0..n {
                let (a, b, c) = (points[i], points[(i + 1) % n], points[(i + 2) % n]);
                let turn = (b - a).cross(c - b);
                if turn != 0.0 {
                    if direction * turn < 0.0 {
                        return false;
                    }
                    direction = turn;
                }
            }
            points.clear();
            true
        };
        for element in shape.elements() {
            match element {
                PathEl::MoveTo(p) => {
                    if !check(&mut points) {
                        return false;
                    }
                    points.push(*p);
                }
                PathEl::LineTo(p) => {
                    if points.last() != Some(p) {
                        points.push(*p);
                    }
                }
                PathEl::ClosePath => {}
                _ => return false,
            }
        }
        if !check(&mut points) {
            return false;
        }
    }
    true
}

fn render_path(scene: &mut Scene, path: &usvg::Path, transform: Affine) {
    if !path.is_visible() {
        return;
    }
    let shape = to_bez_path(path);
    let fill = |scene: &mut Scene| {
        if let Some(fill) = path.fill()
            && let Some((brush, brush_transform)) =
                to_brush(fill.paint(), fill.opacity()).map(|(brush, brush_transform)| {
                    conformal_linear_gradient(brush, brush_transform, transform)
                })
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
            && let Some((brush, brush_transform)) =
                to_brush(stroke.paint(), stroke.opacity()).map(|(brush, brush_transform)| {
                    conformal_linear_gradient(brush, brush_transform, transform)
                })
        {
            scene.stroke(
                &to_stroke(stroke),
                transform,
                &brush,
                Some(brush_transform),
                &shape,
            );
            if let Some(caps) = zero_length_caps(path, stroke) {
                scene.fill(
                    Fill::NonZero,
                    transform,
                    &brush,
                    Some(brush_transform),
                    &caps,
                );
            }
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

/// Converts gradient stops, working around two differences between Vello 0.7 and resvg:
/// Vello starts the first colour at offset 0 whatever its offset, and it interpolates
/// premultiplied colours, while SVG renderers interpolate the colour and the alpha
/// separately. Between stops whose opacities differ, extra stops follow the SVG curve.
fn gradient_stops(stops: &[usvg::Stop], opacity: usvg::Opacity) -> Vec<peniko::ColorStop> {
    /// Extra stops between two stops with different opacities.
    const STEPS: usize = 16;

    let rgba = |stop: &usvg::Stop| {
        let c = stop.color();
        [
            f32::from(c.red),
            f32::from(c.green),
            f32::from(c.blue),
            (stop.opacity() * opacity).get() * 255.0,
        ]
    };
    let color_stop = |offset: f32, [r, g, b, a]: [f32; 4]| peniko::ColorStop {
        offset,
        color: DynamicColor::from_alpha_color(Color::from_rgba8(
            r.round() as u8,
            g.round() as u8,
            b.round() as u8,
            a.round() as u8,
        )),
    };

    let mut out = Vec::with_capacity(stops.len() + 1);
    let Some(first) = stops.first() else {
        return out;
    };
    if first.offset().get() > 0.0 {
        out.push(color_stop(0.0, rgba(first)));
    }
    for (i, stop) in stops.iter().enumerate() {
        if i > 0 {
            let (prev, next) = (rgba(&stops[i - 1]), rgba(stop));
            let (from, to) = (stops[i - 1].offset().get(), stop.offset().get());
            if prev[3] != next[3] && prev[..3] != next[..3] && to > from {
                for step in 1..STEPS {
                    let t = step as f32 / STEPS as f32;
                    let mixed = std::array::from_fn(|c| prev[c] + (next[c] - prev[c]) * t);
                    out.push(color_stop(from + (to - from) * t, mixed));
                }
            }
        }
        out.push(color_stop(stop.offset().get(), rgba(stop)));
    }
    out
}

/// Vello lays out a linear gradient's colour bands at right angles to the line between its
/// transformed end points, which is only right when the transform keeps angles. Under a
/// skew or an uneven scale, this moves the end points to the drawing's own coordinates,
/// where the bands are at right angles to the line, and draws the gradient untransformed.
fn conformal_linear_gradient(
    brush: Brush,
    brush_transform: Affine,
    transform: Affine,
) -> (Brush, Affine) {
    let Brush::Gradient(mut gradient) = brush else {
        return (brush, brush_transform);
    };
    let peniko::GradientKind::Linear(line) = gradient.kind else {
        return (Brush::Gradient(gradient), brush_transform);
    };
    let full = transform * brush_transform;
    let [a, b, c, d, _, _] = full.as_coeffs();
    let keeps_angles = |sign: f64| {
        let tolerance = 1e-9 * (a.abs() + b.abs() + c.abs() + d.abs());
        (a - sign * d).abs() <= tolerance && (b + sign * c).abs() <= tolerance
    };
    let inverse_transform = transform.inverse();
    if keeps_angles(1.0)
        || keeps_angles(-1.0)
        || full.determinant().abs() < 1e-12
        || !inverse_transform.is_finite()
    {
        return (Brush::Gradient(gradient), brush_transform);
    }

    // t(x) = dot(full⁻¹ x - start, dir) / |dir|², an affine function of the output point x.
    let (start, end) = (line.start, line.end);
    let dir = end - start;
    let length_squared = dir.hypot2();
    if length_squared == 0.0 {
        return (Brush::Gradient(gradient), brush_transform);
    }
    let [ia, ib, ic, id, _, _] = full.inverse().as_coeffs();
    // The gradient of t: the transpose of full⁻¹'s linear part times dir.
    let grad =
        vello::kurbo::Vec2::new(ia * dir.x + ib * dir.y, ic * dir.x + id * dir.y) / length_squared;
    let new_start = full * start;
    let new_end = new_start + grad / grad.hypot2();
    gradient.kind = peniko::GradientKind::Linear(peniko::LinearGradientPosition {
        start: new_start,
        end: new_end,
    });
    (Brush::Gradient(gradient), inverse_transform)
}

fn to_brush(paint: &usvg::Paint, opacity: usvg::Opacity) -> Option<(Brush, Affine)> {
    let stops = |stops: &[usvg::Stop]| gradient_stops(stops, opacity);
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
                gr.fr().get(),
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
    fn clip_shapes_are_united() {
        // Merged into one non-zero path, these oppositely wound squares would cancel where
        // they overlap, around (15, 15).
        let image = render(&svg(
            r#"<clipPath id="c"><path d="M0,0 h20 v20 h-20 Z"/><path d="M10,10 v20 h20 v-20 Z"/></clipPath>
                <g clip-path="url(#c)"><rect width="32" height="32"/></g>"#,
        ));
        assert!(is_dark(&image, 15, 15));
        assert!(is_dark(&image, 5, 5));
        assert!(is_dark(&image, 25, 25));
        assert!(!is_dark(&image, 25, 5));
    }

    #[test]
    fn only_shapes_that_cannot_cancel_are_merged() {
        let shapes = |paths: &[&str]| -> Vec<(BezPath, Fill)> {
            paths
                .iter()
                .map(|d| (BezPath::from_svg(d).unwrap(), Fill::NonZero))
                .collect()
        };
        // Same-way rectangles and a convex polygon: merged into one layer.
        assert!(merging_unites(&shapes(&[
            "M0,0 L20,0 L20,20 L0,20 Z",
            "M10,10 L30,10 L30,30 L10,30 Z",
            "M0,40 L10,35 L20,40 L10,50 Z",
        ])));
        // Opposite windings, curves and even-odd rules go through a mask.
        assert!(!merging_unites(&shapes(&[
            "M0,0 L20,0 L20,20 L0,20 Z",
            "M10,10 L10,30 L30,30 L30,10 Z",
        ])));
        assert!(!merging_unites(&shapes(&[
            "M0,0 Q10,10 20,0 Z",
            "M0,0 L5,0 L5,5 Z"
        ])));
        let mut even_odd = shapes(&["M0,0 L20,0 L20,20 Z", "M0,0 L5,0 L5,5 Z"]);
        even_odd[0].1 = Fill::EvenOdd;
        assert!(!merging_unites(&even_odd));
    }

    #[test]
    fn even_odd_clip_rule_is_kept() {
        let image = render(&svg(
            r#"<clipPath id="c"><path d="M0,0 h32 v32 h-32 Z M8,8 h16 v16 h-16 Z" clip-rule="evenodd"/></clipPath>
                <g clip-path="url(#c)"><rect width="32" height="32"/></g>"#,
        ));
        assert!(is_dark(&image, 4, 4));
        assert!(!is_dark(&image, 16, 16));
    }

    #[test]
    fn clip_with_the_area_of_its_bounding_box_still_clips() {
        // A triangle outlined twice has the same signed area as its bounding square, but it
        // is not a rectangle, so the clip must not be skipped.
        let image = render(&svg(
            r#"<clipPath id="c"><path d="M0,0 L32,0 L0,32 Z M0,0 L32,0 L0,32 Z"/></clipPath>
                <g clip-path="url(#c)"><rect width="32" height="32"/></g>"#,
        ));
        assert!(is_dark(&image, 4, 4));
        assert!(!is_dark(&image, 24, 24));
    }

    #[test]
    fn only_axis_aligned_rectangles_count_as_rectangles() {
        let rect = |d: &str| axis_aligned_rect(&BezPath::from_svg(d).unwrap());
        assert_eq!(
            rect("M0,0 L10,0 L10,5 L0,5 Z"),
            Some(Rect::new(0.0, 0.0, 10.0, 5.0))
        );
        assert_eq!(
            rect("M0,0 L10,0 L10,5 L0,5 L0,0 Z"),
            Some(Rect::new(0.0, 0.0, 10.0, 5.0))
        );
        assert_eq!(rect("M0,0 L10,0 L0,10 Z M10,10 L10,0 L0,10 Z"), None);
        assert_eq!(rect("M5,0 L10,5 L5,10 L0,5 Z"), None);
        assert_eq!(
            rect("M0,0 L10,0 L10,5 L0,5 Z M20,0 L30,0 L30,5 L20,5 Z"),
            None
        );
        assert_eq!(rect("M0,0 L10,0 L20,0 L10,0 Z"), None);
    }

    #[test]
    fn caps_on_zero_length_subpaths_are_drawn() {
        let image = render(&svg(
            r#"<path d="M8,16 Z M24,16 L24,16" stroke="black" stroke-width="8" stroke-linecap="round"/>
                <path d="M16,6 Z" stroke="black" stroke-width="6" stroke-linecap="square"/>"#,
        ));
        assert!(is_dark(&image, 8, 16));
        assert!(is_dark(&image, 24, 16));
        // The square cap's corner, which a round cap of the same width wouldn't reach.
        assert!(is_dark(&image, 13, 3));
        assert!(!is_dark(&image, 16, 16));
    }

    #[test]
    fn zero_length_subpaths_are_found() {
        let points = |d: &str| {
            zero_length_subpaths(
                tree(&svg(&format!(r#"<path d="{d}" stroke="black"/>"#)))
                    .root()
                    .children()
                    .iter()
                    .find_map(|n| match n {
                        usvg::Node::Path(p) => Some(p.data().clone()),
                        _ => None,
                    })
                    .as_ref()
                    .unwrap(),
            )
        };
        assert_eq!(
            points("M1,1 L5,1 M3,3 Z M4,4 L4,4"),
            [Point::new(3.0, 3.0), Point::new(4.0, 4.0)]
        );
        assert!(points("M1,1 L5,1 Z").is_empty());
    }

    #[test]
    fn unsupported_features_are_reported() {
        assert!(supports(&tree(&svg(r#"<rect width="8" height="8"/>"#))));
        let unsupported = [
            r#"<pattern id="p" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="2" height="2"/></pattern><rect width="8" height="8" fill="url(#p)"/>"#,
            r#"<mask id="m"><rect width="4" height="4" fill="white"/></mask><rect width="8" height="8" mask="url(#m)"/>"#,
            r#"<filter id="f"><feGaussianBlur stdDeviation="1"/></filter><rect width="8" height="8" filter="url(#f)"/>"#,
            r#"<clipPath id="c" clip-path="url(#d)"><rect width="4" height="4"/></clipPath><clipPath id="d"><rect width="2" height="2"/></clipPath><rect width="8" height="8" clip-path="url(#c)"/>"#,
        ];
        for body in unsupported {
            assert!(!supports(&tree(&svg(body))), "{body}");
        }
    }
}
