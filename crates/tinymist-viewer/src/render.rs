//! todo: gradient/tiling support.
//! todo: develop xilem to support damage tracking.
//! todo: convert a page into a tree of xilem components instead of a single
//! todo: test about text color
//! todo: svg decoder
//! todo: test about images' iccprofile.
//! scene. todo: clean up code.

use std::sync::Arc;

use ecow::EcoVec;
use image::codecs::gif::GifDecoder;
use image::codecs::jpeg::JpegDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::{ImageDecoder, ImageResult, Limits};
use reflexo::{
    hash::Fingerprint,
    vector::{
        ir::{
            self, Abs, Axes, FontIndice, FontItem, FontRef, ImmutStr, Module, PathStyle, Ratio,
            Scalar,
        },
        vm::{GroupContext, RenderVm, TransformContext},
    },
};
use smallvec::SmallVec;
use vello::peniko;
use vello::{
    Scene,
    kurbo::{self, Affine, Vec2},
};

use crate::{GroupScene, VecScene};

pub struct Renderer<'a> {
    module: &'a Module,
}

impl<'a> Renderer<'a> {
    pub fn new(module: &'a Module) -> Self {
        Self { module }
    }
}

impl<'m> RenderVm<'m> for Renderer<'m> {
    type Resultant = Arc<VecScene>;
    type Group = RenderStack;

    fn get_item(&self, value: &Fingerprint) -> Option<&'m ir::VecItem> {
        self.module.get_item(value)
    }

    fn start_group(&mut self, _v: &Fingerprint) -> Self::Group {
        Self::Group {
            kind: GroupKind::General,
            ts: Affine::IDENTITY,
            clipper: None,
            fill: None,
            inner: EcoVec::new(),
        }
    }

    fn start_text(&mut self, value: &Fingerprint, text: &ir::TextItem) -> Self::Group {
        let mut g = self.start_group(value);
        g.kind = GroupKind::Text;
        for style in &text.shape.styles {
            if let ir::PathStyle::Fill(fill) = style {
                g.fill = Some(fill.clone());
            }
        }
        g
    }
}

impl<'m> FontIndice<'m> for Renderer<'m> {
    fn get_font(&self, value: &FontRef) -> Option<&'m ir::FontItem> {
        self.module.fonts.get(value.idx as usize)
    }
}

impl<'m> GlyphFactory for Renderer<'m> {
    fn get_glyph(&mut self, font: &FontItem, glyph: u32, fill: ImmutStr) -> Option<Arc<VecScene>> {
        let glyph_data = font.get_glyph(glyph)?;

        let path = match glyph_data.as_ref() {
            ir::FlatGlyphItem::Outline(path) => svg_path(&path.d)?,
            ir::FlatGlyphItem::Image(..) => return None,
            ir::FlatGlyphItem::None => return None,
        };

        Some(Arc::new(VecScene::Path(
            path,
            peniko::color::parse_color(fill.as_ref())
                .map(|it| it.to_alpha_color())
                .unwrap_or(peniko::Color::BLACK),
        )))
    }
}

/// A stacked builder for [`VecScene`].
///
/// It holds state of the building process.
pub struct RenderStack {
    /// The kind of the group.
    pub kind: GroupKind,
    /// The transform matrix.
    pub ts: Affine,
    /// A unique clip path on stack
    pub clipper: Option<ir::PathItem>,
    /// The fill color.
    pub fill: Option<ImmutStr>,
    /// The inner elements.
    pub inner: EcoVec<(Vec2, Arc<VecScene>)>,
    // /// The bounding box of the group.
    // pub rect: CanvasBBox,
}

#[derive(Debug, Clone, Copy)]
pub enum GroupKind {
    General,
    Text,
}

impl From<RenderStack> for Arc<VecScene> {
    fn from(s: RenderStack) -> Self {
        Arc::new(VecScene::Group(GroupScene {
            // todo: detect whether there is a failure converting paths.
            clip: s.clipper.and_then(|it| svg_path(&it.d)),
            ts: s.ts,
            scenes: s.inner,
        }))
    }
}

/// See [`TransformContext`].
impl<C> TransformContext<C> for RenderStack {
    fn transform_matrix(mut self, _ctx: &mut C, m: &ir::Transform) -> Self {
        let sub_ts = Affine::new([
            m.sx.0 as f64,
            m.ky.0 as f64,
            m.kx.0 as f64,
            m.sy.0 as f64,
            m.tx.0 as f64,
            m.ty.0 as f64,
        ]);
        self.ts *= sub_ts;
        self
    }

    fn transform_translate(mut self, _ctx: &mut C, matrix: Axes<Abs>) -> Self {
        self.ts = self
            .ts
            .then_translate(Vec2::new(matrix.x.0 as f64, matrix.y.0 as f64));
        self
    }

    fn transform_scale(mut self, _ctx: &mut C, x: Ratio, y: Ratio) -> Self {
        self.ts = self.ts.then_scale_non_uniform(x.0 as f64, y.0 as f64);
        self
    }

    fn transform_rotate(mut self, _ctx: &mut C, angle: Scalar) -> Self {
        self.ts *= Affine::rotate(angle.0 as f64);
        self
    }

    fn transform_skew(mut self, _ctx: &mut C, matrix: (Ratio, Ratio)) -> Self {
        self.ts *= Affine::skew(matrix.0.0 as f64, matrix.1.0 as f64);
        self
    }

    fn transform_clip(mut self, _ctx: &mut C, matrix: &ir::PathItem) -> Self {
        self.clipper = Some(matrix.clone());
        self
    }
}

/// See [`GroupContext`].
impl<'m, C: RenderVm<'m, Resultant = Arc<VecScene>> + GlyphFactory> GroupContext<C>
    for RenderStack
{
    fn render_path(&mut self, _ctx: &mut C, path: &ir::PathItem, _abs_ref: &Fingerprint) {
        let mut scene = Scene::new();
        let Some(path_data) = svg_path(&path.d) else {
            return;
        };

        let mut fill_color = peniko::Color::BLACK;
        let mut fill = false;
        let mut fill_rule = peniko::Fill::NonZero;
        let mut stroke_color = peniko::Color::BLACK;
        let mut stroke = false;
        let mut stroke_width = 0f64;
        let mut stroke_join = kurbo::Join::Miter;
        let mut stroke_cap = kurbo::Cap::Butt;
        let mut stroke_miter_limit = 4f64;
        let mut dash_pattern = SmallVec::new();
        let mut dash_offset = 0f64;

        for style in &path.styles {
            match style {
                PathStyle::Fill(color) => {
                    // todo: canvas gradient and pattern
                    if color.starts_with('@') {
                        fill_color = peniko::Color::BLACK;
                    } else {
                        fill_color = peniko::color::parse_color(color.as_ref())
                            .map(|it| it.to_alpha_color())
                            .unwrap_or(peniko::Color::BLACK);
                    }
                    fill = true;
                }
                PathStyle::Stroke(color) => {
                    // todo: canvas gradient and pattern
                    if color.starts_with('@') {
                        stroke_color = peniko::Color::BLACK;
                    } else {
                        stroke_color = peniko::color::parse_color(color.as_ref())
                            .map(|it| it.to_alpha_color())
                            .unwrap_or(peniko::Color::BLACK);
                    }
                    stroke = true;
                }
                PathStyle::StrokeWidth(width) => {
                    stroke_width = width.0 as f64;
                }
                PathStyle::StrokeLineCap(cap) => {
                    stroke_cap = match cap.as_ref() {
                        "butt" => kurbo::Cap::Butt,
                        "round" => kurbo::Cap::Round,
                        "square" => kurbo::Cap::Square,
                        _ => kurbo::Cap::Butt,
                    };
                }
                PathStyle::StrokeLineJoin(join) => {
                    stroke_join = match join.as_ref() {
                        "miter" => kurbo::Join::Miter,
                        "round" => kurbo::Join::Round,
                        "bevel" => kurbo::Join::Bevel,
                        _ => kurbo::Join::Miter,
                    };
                }
                PathStyle::StrokeMitterLimit(limit) => {
                    stroke_miter_limit = limit.0 as f64;
                }
                PathStyle::StrokeDashArray(array) => {
                    dash_pattern = array.iter().map(|d| d.0 as f64).collect();
                }
                PathStyle::StrokeDashOffset(offset) => {
                    dash_offset = offset.0 as f64;
                }
                PathStyle::FillRule(rule) => {
                    fill_rule = match rule.as_ref() {
                        "nonzero" => peniko::Fill::NonZero,
                        "evenodd" => peniko::Fill::EvenOdd,
                        _ => peniko::Fill::NonZero,
                    };
                }
            }
        }

        if fill {
            //  Affine::IDENTITY
            let brush_transform = None;
            // let brush_transform =
            //     (!transform.is_identity()).then_some(convert_transform(transform));
            // todo: paint transform?
            // shape_paint_transform(state, paint, shape);
            // let size = shape_fill_size(state, paint, shape);
            // let brush = convert_paint_to_brush(paint, size);

            scene.fill(
                fill_rule,
                Affine::IDENTITY,
                &peniko::Brush::Solid(fill_color),
                brush_transform,
                &path_data,
            );
        }

        if stroke {
            let brush_transform = None;

            let mut kurbo_stroke = kurbo::Stroke {
                width: stroke_width,
                join: stroke_join,
                miter_limit: stroke_miter_limit,
                start_cap: stroke_cap,
                end_cap: stroke_cap,
                ..Default::default()
            };
            if !dash_pattern.is_empty() {
                kurbo_stroke.dash_pattern = dash_pattern;
                kurbo_stroke.dash_offset = dash_offset;
            }

            scene.stroke(
                &kurbo_stroke,
                Affine::IDENTITY,
                &peniko::Brush::Solid(stroke_color),
                brush_transform,
                &path_data,
            );
        }

        self.inner.push((
            Vec2::new(0., 0.),
            Arc::new(VecScene::Scene(Box::new(scene), None)),
        ));
    }

    fn render_image(&mut self, _ctx: &mut C, image_item: &ir::ImageItem) {
        let mut scene = vello::Scene::new();

        let width = image_item.image.width();
        let height = image_item.image.height();

        if width == 0 || height == 0 || image_item.size.x.0 < 1e-11 || image_item.size.y.0 < 1e-11 {
            return;
        }

        let data = std::io::Cursor::new(&image_item.image.data);

        let image_data = match image_item.image.format.as_ref() {
            "jpeg" => decode(JpegDecoder::new(data)),
            "png" => decode(PngDecoder::new(data)),
            "webp" => decode(WebPDecoder::new(data)),
            "gif" => decode(GifDecoder::new(data)),
            // todo: svg
            // "svg+xml" => decode(SvgDecoder::new(data)),
            _ => return,
        };
        let Ok(image_data) = image_data else {
            return;
        };

        let image_data = peniko::ImageData {
            data: peniko::Blob::new(Arc::new(image_data.to_rgba8().into_vec())),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::Alpha,
            width,
            height,
        };

        let brush = peniko::ImageBrush::new(image_data);
        scene.draw_image(&brush, kurbo::Affine::IDENTITY);

        let transform = Affine::IDENTITY.pre_scale_non_uniform(
            image_item.size.x.0 as f64 / width as f64,
            image_item.size.y.0 as f64 / height as f64,
        );

        self.inner.push((
            Vec2::new(0., 0.),
            Arc::new(VecScene::Scene(Box::new(scene), Some(transform))),
        ));

        fn decode<T: ImageDecoder>(decoder: ImageResult<T>) -> ImageResult<image::DynamicImage> {
            let mut decoder = decoder?;
            decoder.set_limits(Limits::default())?;
            let dynamic = image::DynamicImage::from_decoder(decoder)?;
            Ok(dynamic)
        }
    }

    fn render_item_at(&mut self, ctx: &mut C, pos: ir::Point, item: &Fingerprint) {
        self.inner.push((
            Vec2::new(pos.x.0 as f64, pos.y.0 as f64),
            ctx.render_item(item),
        ));
    }

    fn render_glyph(&mut self, ctx: &mut C, pos: Axes<Scalar>, font: &FontItem, glyph: u32) {
        if let Some(glyph) = ctx.get_glyph(font, glyph, self.fill.clone().unwrap()) {
            self.inner
                .push((Vec2::new(pos.x.0 as f64, pos.y.0 as f64), glyph));
        }
    }
}

trait GlyphFactory {
    fn get_glyph(&mut self, font: &FontItem, glyph: u32, fill: ImmutStr) -> Option<Arc<VecScene>>;
}

/// Converts an SVG path to a [`kurbo::BezPath`].
fn svg_path(path_data: &str) -> Option<kurbo::BezPath> {
    let mut builder = GlyphPathBuilder::default();
    for segment in svgtypes::SimplifyingPathParser::from(path_data) {
        let segment = match segment {
            Ok(v) => v,
            Err(_) => break,
        };

        match segment {
            svgtypes::SimplePathSegment::MoveTo { x, y } => {
                builder.move_to(x as f32, y as f32);
            }
            svgtypes::SimplePathSegment::LineTo { x, y } => {
                builder.line_to(x as f32, y as f32);
            }
            svgtypes::SimplePathSegment::Quadratic { x1, y1, x, y } => {
                builder.quad_to(x1 as f32, y1 as f32, x as f32, y as f32);
            }
            svgtypes::SimplePathSegment::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                builder.curve_to(
                    x1 as f32, y1 as f32, x2 as f32, y2 as f32, x as f32, y as f32,
                );
            }
            svgtypes::SimplePathSegment::ClosePath => {
                builder.close();
            }
        }
    }

    Some(builder.0)
}

#[derive(Default)]
pub struct GlyphPathBuilder(kurbo::BezPath);

impl GlyphPathBuilder {
    // Y axis is inverted.
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((x as f64, y as f64));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((x as f64, y as f64));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to((x1 as f64, y1 as f64), (x as f64, y as f64));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.curve_to(
            (x1 as f64, y1 as f64),
            (x2 as f64, y2 as f64),
            (x as f64, y as f64),
        );
    }

    fn close(&mut self) {
        self.0.close_path();
    }
}
