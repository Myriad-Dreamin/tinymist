use std::f32::consts::TAU;

use typst_library::layout::{Quadrant, Size, Transform};
use typst_library::visualize as viz;
use vello::{kurbo, peniko};

pub fn convert_fixed_stroke(stroke: &viz::FixedStroke) -> kurbo::Stroke {
    let width = stroke.thickness.to_pt();
    let join = match stroke.join {
        viz::LineJoin::Miter => kurbo::Join::Miter,
        viz::LineJoin::Round => kurbo::Join::Round,
        viz::LineJoin::Bevel => kurbo::Join::Bevel,
    };
    let miter_limit = stroke.miter_limit.get();
    let cap = match stroke.cap {
        viz::LineCap::Butt => kurbo::Cap::Butt,
        viz::LineCap::Round => kurbo::Cap::Round,
        viz::LineCap::Square => kurbo::Cap::Square,
    };

    let mut kurbo_stroke = kurbo::Stroke {
        width,
        join,
        miter_limit,
        start_cap: cap,
        end_cap: cap,
        ..Default::default()
    };

    if let Some(dash) = &stroke.dash {
        kurbo_stroke.dash_pattern = dash.array.iter().map(|dash| dash.to_pt()).collect();
        kurbo_stroke.dash_offset = dash.phase.to_pt();
    }

    kurbo_stroke
}

pub fn convert_paint_to_brush(paint: &viz::Paint, size: Size) -> peniko::Brush {
    match paint {
        viz::Paint::Solid(solid) => peniko::Brush::Solid(convert_color(solid)),
        viz::Paint::Gradient(gradient) => {
            let ratio = size.aspect_ratio();

            let stops = gradient
                .stops_ref()
                .iter()
                .map(|(color, ratio)| peniko::ColorStop {
                    offset: ratio.get() as f32,
                    color: peniko::color::DynamicColor::from_alpha_color(convert_color(color)),
                })
                .collect::<Vec<_>>();

            let gradient = match gradient {
                viz::Gradient::Linear(linear) => {
                    let angle = viz::Gradient::correct_aspect_ratio(linear.angle, ratio);
                    let (sin, cos) = (angle.sin(), angle.cos());
                    let length = sin.abs() + cos.abs();
                    let (start, end) = match angle.quadrant() {
                        Quadrant::First => ((0.0, 0.0), (cos * length, sin * length)),
                        Quadrant::Second => ((1.0, 0.0), (cos * length + 1.0, sin * length)),
                        Quadrant::Third => ((1.0, 1.0), (cos * length + 1.0, sin * length + 1.0)),
                        Quadrant::Fourth => ((0.0, 1.0), (cos * length, sin * length + 1.0)),
                    };

                    peniko::Gradient::new_linear(start, end).with_stops(stops.as_slice())
                }
                viz::Gradient::Radial(radial) => {
                    let start_center = (radial.focal_center.x.get(), radial.focal_center.y.get());
                    let start_radius = radial.focal_radius.get() as f32;
                    let end_center = (radial.center.x.get(), radial.center.y.get());
                    let end_radius = radial.radius.get() as f32;

                    peniko::Gradient::new_two_point_radial(
                        start_center,
                        start_radius,
                        end_center,
                        end_radius,
                    )
                    .with_stops(stops.as_slice())
                }
                viz::Gradient::Conic(conic) => {
                    let angle = -(viz::Gradient::correct_aspect_ratio(conic.angle, ratio).to_rad()
                        as f32)
                        .rem_euclid(TAU);
                    let center = (conic.center.x.get(), conic.center.y.get());

                    peniko::Gradient::new_sweep(center, angle, TAU - angle)
                        .with_stops(stops.as_slice())
                }
            };
            peniko::Brush::Gradient(gradient)
        }
        // TODO: Support pattern.
        viz::Paint::Tiling(_) => peniko::Brush::Solid(peniko::Color::new([1.0, 0.0, 0.0, 0.0])),
    }
}

pub fn convert_color(color: &viz::Color) -> peniko::Color {
    let channels = color.to_vec4_u8();
    peniko::Color::from_rgba8(channels[0], channels[1], channels[2], channels[3])
}

pub fn convert_transform(transform: Transform) -> kurbo::Affine {
    kurbo::Affine::new([
        transform.sx.get(),
        transform.ky.get(),
        transform.kx.get(),
        transform.sy.get(),
        transform.tx.to_pt(),
        transform.ty.to_pt(),
    ])
}
