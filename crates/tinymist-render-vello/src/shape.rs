use kurbo::Shape;
use typst_library::layout::{Abs, Ratio, Size, Transform};
use typst_library::visualize as viz;
use vello::{kurbo, peniko};

use crate::{RenderState, utils::*};

#[derive(Default, Debug, Clone)]
pub struct ShapeScene {
    pub transform: kurbo::Affine,
    pub path: kurbo::BezPath,
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
}

#[derive(Debug, Clone)]
pub struct Fill {
    pub style: peniko::Fill,
    pub brush: peniko::Brush,
    pub transform: Option<kurbo::Affine>,
}

#[derive(Default, Debug, Clone)]
pub struct Stroke {
    pub style: kurbo::Stroke,
    pub brush: peniko::Brush,
    pub transform: Option<kurbo::Affine>,
}

impl ShapeScene {
    pub fn render(&self, scene: &mut vello::Scene) {
        if let Some(fill) = &self.fill {
            scene.fill(
                fill.style,
                self.transform,
                &fill.brush,
                fill.transform,
                &self.path,
            );
        }

        if let Some(stroke) = &self.stroke {
            scene.stroke(
                &stroke.style,
                self.transform,
                &stroke.brush,
                stroke.transform,
                &self.path,
            );
        }
    }
}

pub fn render_shape(
    shape: &viz::Shape,
    state: RenderState,
    local_transform: Transform,
) -> ShapeScene {
    ShapeScene {
        transform: convert_transform(local_transform),
        path: convert_geometry_to_path(&shape.geometry),
        fill: shape.fill.as_ref().map(|paint| {
            let transform = shape_paint_transform(state, paint, shape);
            let size = shape_fill_size(state, paint, shape);
            let brush = convert_paint_to_brush(paint, size);

            Fill {
                style: match shape.fill_rule {
                    viz::FillRule::NonZero => peniko::Fill::NonZero,
                    viz::FillRule::EvenOdd => peniko::Fill::EvenOdd,
                },
                brush,
                transform: (!transform.is_identity()).then_some(convert_transform(transform)),
            }
        }),
        stroke: shape.stroke.as_ref().map(|stroke| {
            let transform = shape_paint_transform(state, &stroke.paint, shape);
            let size = shape_fill_size(state, &stroke.paint, shape);
            let brush = convert_paint_to_brush(&stroke.paint, size);

            Stroke {
                style: convert_fixed_stroke(stroke),
                brush,
                transform: (!transform.is_identity()).then_some(convert_transform(transform)),
            }
        }),
    }
}

/// Calculate the transform of the shape's fill or stroke.
pub fn shape_paint_transform(
    state: RenderState,
    paint: &viz::Paint,
    shape: &viz::Shape,
) -> Transform {
    let mut shape_size = shape.geometry.bbox_size();
    // Edge cases for strokes.
    if shape_size.x.to_pt() == 0.0 {
        shape_size.x = Abs::pt(1.0);
    }

    if shape_size.y.to_pt() == 0.0 {
        shape_size.y = Abs::pt(1.0);
    }

    if let viz::Paint::Gradient(gradient) = paint {
        match gradient.unwrap_relative(false) {
            viz::RelativeTo::Self_ => Transform::scale(
                Ratio::new(shape_size.x.to_pt()),
                Ratio::new(shape_size.y.to_pt()),
            ),
            viz::RelativeTo::Parent => Transform::scale(
                Ratio::new(state.size.x.to_pt()),
                Ratio::new(state.size.y.to_pt()),
            )
            .post_concat(state.transform.invert().unwrap()),
        }
    } else if let viz::Paint::Tiling(tiling) = paint {
        match tiling.unwrap_relative(false) {
            viz::RelativeTo::Self_ => Transform::identity(),
            viz::RelativeTo::Parent => state.transform.invert().unwrap(),
        }
    } else {
        Transform::identity()
    }
}

/// Calculate the size of the shape's fill.
fn shape_fill_size(state: RenderState, paint: &viz::Paint, shape: &viz::Shape) -> Size {
    let mut shape_size = shape.geometry.bbox_size();
    // Edge cases for strokes.
    if shape_size.x.to_pt() == 0.0 {
        shape_size.x = Abs::pt(1.0);
    }

    if shape_size.y.to_pt() == 0.0 {
        shape_size.y = Abs::pt(1.0);
    }

    if let viz::Paint::Gradient(gradient) = paint {
        match gradient.unwrap_relative(false) {
            viz::RelativeTo::Self_ => shape_size,
            viz::RelativeTo::Parent => state.size,
        }
    } else {
        shape_size
    }
}

pub fn convert_geometry_to_path(geometry: &viz::Geometry) -> kurbo::BezPath {
    match geometry {
        viz::Geometry::Line(p) => {
            kurbo::Line::new((0.0, 0.0), (p.x.to_pt(), p.y.to_pt())).to_path(0.01)
        }
        viz::Geometry::Rect(rect) => {
            kurbo::Rect::from_origin_size((0.0, 0.0), (rect.x.to_pt(), rect.y.to_pt()))
                .to_path(0.01)
        }

        viz::Geometry::Curve(curve) => convert_curve(curve),
    }
}

pub fn convert_curve(path: &viz::Curve) -> kurbo::BezPath {
    let mut bezpath = kurbo::BezPath::new();

    for item in &path.0 {
        match item {
            viz::CurveItem::Move(p) => bezpath.move_to((p.x.to_pt(), p.y.to_pt())),
            viz::CurveItem::Line(p) => bezpath.line_to((p.x.to_pt(), p.y.to_pt())),
            viz::CurveItem::Cubic(p1, p2, p3) => bezpath.curve_to(
                (p1.x.to_pt(), p1.y.to_pt()),
                (p2.x.to_pt(), p2.y.to_pt()),
                (p3.x.to_pt(), p3.y.to_pt()),
            ),
            viz::CurveItem::Close => bezpath.close_path(),
        }
    }
    bezpath
}
