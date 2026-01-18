use ttf_parser::{GlyphId, OutlineBuilder};
use typst_library::layout::{Abs, Ratio, Transform};
use typst_library::text::{Font, TextItem};
use typst_library::visualize as viz;
use vello::{kurbo, peniko};

use crate::{
    RenderState,
    utils::{convert_fixed_stroke, convert_paint_to_brush, convert_transform},
};

#[derive(Default, Debug, Clone)]
pub struct TextScene {
    pub transform: Option<kurbo::Affine>,
    pub paths: Vec<GlyphPath>,
    pub fill: TextFill,
    pub stroke: Option<TextStroke>,
}

impl TextScene {
    pub fn render(&self, scene: &mut vello::Scene) {
        let mut text_scene = vello::Scene::new();

        for path in self.paths.iter() {
            text_scene.fill(
                peniko::Fill::NonZero,
                path.transform,
                &self.fill.brush,
                path.fill_transform,
                &path.path,
            );

            if let Some(stroke) = &self.stroke {
                text_scene.stroke(
                    &stroke.style,
                    path.transform,
                    &stroke.brush,
                    path.stroke_transform,
                    &path.path,
                );
            }
        }

        scene.append(&text_scene, self.transform);
    }
}

/// [`BezPath`][kurbo::BezPath] of a single text glyph.
#[derive(Default, Debug, Clone)]
pub struct GlyphPath {
    pub transform: kurbo::Affine,
    pub path: kurbo::BezPath,
    pub fill_transform: Option<kurbo::Affine>,
    pub stroke_transform: Option<kurbo::Affine>,
}

/// Fill of the entire [`TextScene`].
#[derive(Default, Debug, Clone)]
pub struct TextFill {
    pub brush: peniko::Brush,
}

/// Stroke of the entire [`TextScene`].
#[derive(Default, Debug, Clone)]
pub struct TextStroke {
    pub style: kurbo::Stroke,
    pub brush: peniko::Brush,
}

pub fn render_text(
    text: &TextItem,
    state: RenderState,
    local_transform: Option<Transform>,
) -> TextScene {
    let mut text_scene = TextScene {
        transform: local_transform.map(|transform| {
            convert_transform(transform.pre_concat(Transform::scale(Ratio::one(), -Ratio::one())))
        }),
        fill: TextFill {
            brush: convert_paint_to_brush(&text.fill, state.size),
        },
        stroke: text.stroke.as_ref().map(|stroke| TextStroke {
            style: convert_fixed_stroke(stroke),
            brush: convert_paint_to_brush(&stroke.paint, state.size),
        }),
        ..Default::default()
    };

    let scale = text.size.to_pt() / text.font.units_per_em();

    let mut x = 0.0;
    let mut offset_transform = Transform::identity();

    for glyph in &text.glyphs {
        let id = GlyphId(glyph.id);
        let offset = x + glyph.x_offset.at(text.size).to_pt();

        offset_transform.tx = Abs::pt(offset);

        let glyph_path = render_outline_glyph(
            text,
            state
                .pre_concat(Transform::scale(Ratio::one(), -Ratio::one()))
                .pre_translate(typst_library::layout::Point::new(
                    Abs::pt(offset),
                    Abs::zero(),
                )),
            id,
            scale,
            offset_transform,
        );

        if let Some(glyph_path) = glyph_path {
            text_scene.paths.push(glyph_path);
        }

        x += glyph.x_advance.at(text.size).to_pt();
    }

    text_scene
}

fn render_outline_glyph(
    text: &TextItem,
    state: RenderState,
    glyph_id: GlyphId,
    scale: f64,
    local_transform: Transform,
) -> Option<GlyphPath> {
    Some(GlyphPath {
        transform: convert_transform(local_transform),
        path: convert_outline_glyph_to_path(&text.font, glyph_id, scale)?,
        fill_transform: {
            let transform = text_paint_transform(state, &text.fill);
            (!transform.is_identity()).then_some(convert_transform(transform))
        },
        stroke_transform: text
            .stroke
            .as_ref()
            .map(|stroke| text_paint_transform(state, &stroke.paint))
            .filter(|transform| !transform.is_identity())
            .map(convert_transform),
    })
}

fn text_paint_transform(state: RenderState, paint: &viz::Paint) -> Transform {
    match paint {
        viz::Paint::Solid(_) => Transform::identity(),
        viz::Paint::Gradient(gradient) => match gradient.unwrap_relative(true) {
            viz::RelativeTo::Self_ => Transform::identity(),
            viz::RelativeTo::Parent => Transform::scale(
                Ratio::new(state.size.x.to_pt()),
                Ratio::new(state.size.y.to_pt()),
            )
            .post_concat(state.transform.invert().unwrap()),
        },
        viz::Paint::Tiling(tiling) => match tiling.unwrap_relative(true) {
            viz::RelativeTo::Self_ => Transform::identity(),
            viz::RelativeTo::Parent => state.transform.invert().unwrap(),
        },
    }
}

fn convert_outline_glyph_to_path(font: &Font, id: GlyphId, scale: f64) -> Option<kurbo::BezPath> {
    let mut builder = GlyphPathBuilder(kurbo::BezPath::new(), scale);
    font.ttf().outline_glyph(id, &mut builder)?;
    Some(builder.0)
}

pub struct GlyphPathBuilder(kurbo::BezPath, f64);

impl GlyphPathBuilder {
    pub fn path(&self) -> &kurbo::BezPath {
        &self.0
    }

    pub fn path_mut(&mut self) -> &mut kurbo::BezPath {
        &mut self.0
    }

    pub fn scale(&self) -> f64 {
        self.1
    }
}

impl OutlineBuilder for GlyphPathBuilder {
    // Y axis is inverted.
    fn move_to(&mut self, x: f32, y: f32) {
        let scale = self.scale();
        self.path_mut()
            .move_to((scale * x as f64, scale * y as f64));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let scale = self.scale();
        self.path_mut()
            .line_to((scale * x as f64, scale * y as f64));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let scale = self.scale();
        self.path_mut().quad_to(
            (scale * x1 as f64, scale * y1 as f64),
            (scale * x as f64, scale * y as f64),
        );
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let scale = self.scale();
        self.path_mut().curve_to(
            (scale * x1 as f64, scale * y1 as f64),
            (scale * x2 as f64, scale * y2 as f64),
            (scale * x as f64, scale * y as f64),
        );
    }

    fn close(&mut self) {
        self.path_mut().close_path();
    }
}
