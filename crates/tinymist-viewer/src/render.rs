//! todo: develop xilem to support damage tracking.
//! todo: convert a page into a tree of xilem components instead of a single
//! todo: test about text color
//! todo: test about images' iccprofile.
//! scene. todo: clean up code.

use std::sync::{Arc, OnceLock};

use ecow::EcoVec;
use image::codecs::gif::GifDecoder;
use image::codecs::jpeg::JpegDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::imageops::FilterType;
use image::{ImageDecoder, ImageResult, Limits};
use reflexo::{
    hash::Fingerprint,
    vector::{
        ir::{
            self, Abs, Axes, ColorSpace, FontIndice, FontItem, FontRef, GradientItem, GradientKind,
            GradientStyle, ImmutStr, Module, PathStyle, PatternItem, Ratio, Scalar,
        },
        vm::{GroupContext, RenderVm, TransformContext},
    },
};
use smallvec::SmallVec;
use typst::visualize::{Color as TypstColor, ColorSpace as TypstColorSpace, WeightedColor};
use vello::peniko;
use vello::{
    Scene,
    kurbo::{self, Affine, Rect, Shape, Vec2},
};

use crate::{
    GroupScene, GroupSceneItem, PageAccessibility, SvgResource, SvgResourceFormat,
    SvgResourceResolver, VecScene,
};

pub struct Renderer<'a> {
    module: &'a Module,
    svg_resource_resolver: Option<Arc<dyn SvgResourceResolver>>,
}

impl<'a> Renderer<'a> {
    pub fn new(module: &'a Module) -> Self {
        Self {
            module,
            svg_resource_resolver: None,
        }
    }

    pub fn with_svg_resource_resolver(
        mut self,
        resolver: Option<Arc<dyn SvgResourceResolver>>,
    ) -> Self {
        self.svg_resource_resolver = resolver;
        self
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
            glyph_style: None,
            items: EcoVec::new(),
        }
    }

    fn start_text(&mut self, value: &Fingerprint, text: &ir::TextItem) -> Self::Group {
        let mut g = self.start_group(value);
        g.kind = GroupKind::Text;
        let stroke_scale = self
            .get_font(&text.shape.font)
            .map(|font| f64::from(font.units_per_em.0) / f64::from(text.shape.size.0))
            .filter(|scale| scale.is_finite())
            .unwrap_or(1.0);
        g.glyph_style = Some(resolve_draw_style(self, &text.shape.styles, stroke_scale));
        g
    }
}

impl<'m> FontIndice<'m> for Renderer<'m> {
    fn get_font(&self, value: &FontRef) -> Option<&'m ir::FontItem> {
        self.module.fonts.get(value.idx as usize)
    }
}

impl<'m> GlyphFactory for Renderer<'m> {
    fn get_glyph(
        &mut self,
        font: &FontItem,
        glyph: u32,
        style: &DrawStyle,
        pos: Vec2,
    ) -> Option<Arc<VecScene>> {
        let glyph_data = font.get_glyph(glyph)?;

        match glyph_data.as_ref() {
            ir::FlatGlyphItem::Outline(path) => {
                let path = svg_path(&path.d)?;
                let style = style.translated_for_glyph(pos);
                if !style.has_draw() {
                    return None;
                }

                let mut scene = Scene::new();
                render_path_with_style(self, &mut scene, &path, &style);
                Some(Arc::new(VecScene::Scene(Box::new(scene), None)))
            }
            ir::FlatGlyphItem::Image(glyph) => {
                let (scene, image_transform) =
                    image_item_scene(&glyph.image, self.svg_resource_resolver())?;
                Some(Arc::new(VecScene::Scene(
                    Box::new(scene),
                    Some(transform_to_affine(&glyph.ts) * image_transform),
                )))
            }
            ir::FlatGlyphItem::None => None,
        }
    }

    fn resolve_paint(&self, paint: &ImmutStr) -> PaintBrush {
        resolve_paint(self.module, paint)
    }

    fn svg_resource_resolver(&self) -> Option<&dyn SvgResourceResolver> {
        self.svg_resource_resolver.as_deref()
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
    /// The draw style for glyph outlines in text groups.
    glyph_style: Option<DrawStyle>,
    /// The ordered visual and semantic items.
    pub items: EcoVec<GroupSceneItem>,
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
            items: s.items,
        }))
    }
}

/// See [`TransformContext`].
impl<C> TransformContext<C> for RenderStack {
    fn transform_matrix(mut self, _ctx: &mut C, m: &ir::Transform) -> Self {
        let sub_ts = transform_to_affine(m);
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
    fn render_path(&mut self, ctx: &mut C, path: &ir::PathItem, _abs_ref: &Fingerprint) {
        let mut scene = Scene::new();
        let Some(path_data) = svg_path(&path.d) else {
            return;
        };
        let style = resolve_draw_style(ctx, &path.styles, 1.0);
        render_path_with_style(ctx, &mut scene, &path_data, &style);

        self.items.push(GroupSceneItem::Scene {
            pos: Vec2::new(0., 0.),
            scene: Arc::new(VecScene::Scene(Box::new(scene), None)),
        });
    }

    fn render_link(&mut self, _ctx: &mut C, link: &ir::LinkItem) {
        if link.size.x.0 <= 0.0 || link.size.y.0 <= 0.0 {
            return;
        }

        self.items
            .push(GroupSceneItem::Accessibility(PageAccessibility::link_node(
                link.href.as_ref(),
                masonry::accesskit::Rect {
                    x0: 0.0,
                    y0: 0.0,
                    x1: link.size.x.0 as f64,
                    y1: link.size.y.0 as f64,
                },
            )));
    }

    fn render_image(&mut self, ctx: &mut C, image_item: &ir::ImageItem) {
        if image_item.image.width() == 0
            || image_item.image.height() == 0
            || image_item.size.x.0 < 1e-11
            || image_item.size.y.0 < 1e-11
        {
            return;
        }

        let Some((scene, transform)) = image_item_scene(image_item, ctx.svg_resource_resolver())
        else {
            return;
        };

        self.items.push(GroupSceneItem::Scene {
            pos: Vec2::new(0., 0.),
            scene: Arc::new(VecScene::Scene(Box::new(scene), Some(transform))),
        });
    }

    fn render_item_at(&mut self, ctx: &mut C, pos: ir::Point, item: &Fingerprint) {
        self.items.push(GroupSceneItem::Scene {
            pos: Vec2::new(pos.x.0 as f64, pos.y.0 as f64),
            scene: ctx.render_item(item),
        });
    }

    fn render_glyph(&mut self, ctx: &mut C, pos: Axes<Scalar>, font: &FontItem, glyph: u32) {
        let pos = Vec2::new(pos.x.0 as f64, pos.y.0 as f64);
        if let Some(style) = &self.glyph_style
            && let Some(glyph) = ctx.get_glyph(font, glyph, style, pos)
        {
            self.items.push(GroupSceneItem::Scene { pos, scene: glyph });
        }
    }
}

fn image_item_scene(
    image_item: &ir::ImageItem,
    svg_resource_resolver: Option<&dyn SvgResourceResolver>,
) -> Option<(vello::Scene, Affine)> {
    let (image_data, width, height, quality) = decode_image_data_for_item(
        image_item.image.format.as_ref(),
        &image_item.image.data,
        image_item.size,
        &image_item.image.attrs,
        svg_resource_resolver,
    )?;

    let mut scene = vello::Scene::new();
    let brush = peniko::ImageBrush::new(image_data).with_quality(quality);
    scene.draw_image(&brush, kurbo::Affine::IDENTITY);

    let transform = Affine::IDENTITY.pre_scale_non_uniform(
        image_item.size.x.0 as f64 / width as f64,
        image_item.size.y.0 as f64 / height as f64,
    );

    Some((scene, transform))
}

fn decode_image_data_for_item(
    format: &str,
    data: &[u8],
    target_size: Axes<Abs>,
    attrs: &[ir::ImageAttr],
    svg_resource_resolver: Option<&dyn SvgResourceResolver>,
) -> Option<(peniko::ImageData, u32, u32, peniko::ImageQuality)> {
    match format {
        "svg" | "svg+xml" => {
            let (image_data, width, height) =
                decode_svg_image(data, target_size, svg_resource_resolver)?;
            Some((image_data, width, height, peniko::ImageQuality::Low))
        }
        _ => {
            let (image_data, width, height) =
                decode_raster_image(format, data, target_size, is_pixelated(attrs))?;
            Some((image_data, width, height, peniko::ImageQuality::Low))
        }
    }
}

fn is_pixelated(attrs: &[ir::ImageAttr]) -> bool {
    attrs.iter().any(|attr| {
        matches!(attr, ir::ImageAttr::ImageRendering(rendering) if rendering.as_ref() == "pixelated")
    })
}

fn decode_svg_image(
    data: &[u8],
    target_size: Axes<Abs>,
    svg_resource_resolver: Option<&dyn SvgResourceResolver>,
) -> Option<(peniko::ImageData, u32, u32)> {
    let svg = std::str::from_utf8(data).ok()?;
    let options = svg_options(data, svg_resource_resolver);
    let tree = resvg::usvg::Tree::from_str(svg, &options).ok()?;
    let tree_size = tree.size();
    let width = tree_size.width();
    let height = tree_size.height();
    if width == 0.0 || height == 0.0 {
        return None;
    }

    let (width, height) = target_texture_size(target_size, width as f64 / height as f64);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    let transform = resvg::tiny_skia::Transform::from_scale(
        width as f32 / tree_size.width(),
        height as f32 / tree_size.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    Some((
        peniko::ImageData {
            data: peniko::Blob::new(Arc::new(pixmap.data().to_vec())),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::AlphaPremultiplied,
            width,
            height,
        },
        width,
        height,
    ))
}

fn target_texture_size(target_size: Axes<Abs>, aspect: f64) -> (u32, u32) {
    let width = (target_size.x.0 as f64)
        .max(aspect * target_size.y.0 as f64)
        .ceil()
        .max(1.0) as u32;
    let height = (width as f64 / aspect).ceil().max(1.0) as u32;
    (width, height)
}

fn svg_options<'a>(
    svg_data: &'a [u8],
    svg_resource_resolver: Option<&'a dyn SvgResourceResolver>,
) -> resvg::usvg::Options<'a> {
    let resolve_string = resvg::usvg::ImageHrefResolver::default_string_resolver();
    // todo: improve svg_options with user font settings.
    resvg::usvg::Options {
        font_family: "Libertinus Serif".to_owned(),
        fontdb: svg_fontdb(),
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: resvg::usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(move |href, options| {
                let href = href.strip_prefix("file://").unwrap_or(href);
                if let Some(resource) = svg_resource_resolver
                    .and_then(|resolver| resolver.resolve_svg_resource(svg_data, href))
                {
                    return Some(svg_resource_to_usvg_image_kind(resource));
                }
                resolve_string(href, options)
            }),
        },
        ..Default::default()
    }
}

fn svg_resource_to_usvg_image_kind(resource: SvgResource) -> resvg::usvg::ImageKind {
    match resource.format {
        SvgResourceFormat::Jpeg => resvg::usvg::ImageKind::JPEG(resource.data),
        SvgResourceFormat::Png => resvg::usvg::ImageKind::PNG(resource.data),
        SvgResourceFormat::Gif => resvg::usvg::ImageKind::GIF(resource.data),
        SvgResourceFormat::Webp => resvg::usvg::ImageKind::WEBP(resource.data),
    }
}

fn transform_to_affine(m: &ir::Transform) -> Affine {
    Affine::new([
        m.sx.0 as f64,
        m.ky.0 as f64,
        m.kx.0 as f64,
        m.sy.0 as f64,
        m.tx.0 as f64,
        m.ty.0 as f64,
    ])
}

fn svg_fontdb() -> Arc<resvg::usvg::fontdb::Database> {
    static FONT_DB: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    Arc::clone(FONT_DB.get_or_init(|| {
        let mut database = resvg::usvg::fontdb::Database::new();
        for data in typst_assets::fonts() {
            database.load_font_data(data.to_vec());
        }
        database.set_serif_family("Libertinus Serif");
        database.set_sans_serif_family("New Computer Modern");
        database.set_monospace_family("DejaVu Sans Mono");
        Arc::new(database)
    }))
}

fn decode_raster_image(
    format: &str,
    data: &[u8],
    target_size: Axes<Abs>,
    pixelated: bool,
) -> Option<(peniko::ImageData, u32, u32)> {
    let data = std::io::Cursor::new(data);

    let decoded = match format {
        "jpeg" | "jpg" => decode(JpegDecoder::new(data)),
        "png" => decode(PngDecoder::new(data)),
        "webp" => decode(WebPDecoder::new(data)),
        "gif" => decode(GifDecoder::new(data)),
        _ => return None,
    };
    let Ok(image_data) = decoded else {
        return None;
    };

    let width = image_data.width();
    let height = image_data.height();
    if width == 0 || height == 0 {
        return None;
    }

    let image_data = resize_raster_image(image_data, target_size, pixelated);
    let width = image_data.width();
    let height = image_data.height();

    Some((
        peniko::ImageData {
            data: peniko::Blob::new(Arc::new(image_data.to_rgba8().into_vec())),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::Alpha,
            width,
            height,
        },
        width,
        height,
    ))
}

fn resize_raster_image(
    image_data: image::DynamicImage,
    target_size: Axes<Abs>,
    pixelated: bool,
) -> image::DynamicImage {
    let (width, height) = target_texture_size(
        target_size,
        image_data.width() as f64 / image_data.height() as f64,
    );

    if (width, height) == (image_data.width(), image_data.height()) {
        return image_data;
    }

    let upscale = width > image_data.width();
    let filter = match (pixelated, upscale) {
        (true, _) => FilterType::Nearest,
        (false, true) => FilterType::CatmullRom,
        (false, false) => FilterType::Lanczos3,
    };

    image_data.resize_exact(width, height, filter)
}

fn decode<T: ImageDecoder>(decoder: ImageResult<T>) -> ImageResult<image::DynamicImage> {
    let mut decoder = decoder?;
    decoder.set_limits(Limits::default())?;
    let orientation = decoder.orientation()?;
    let mut dynamic = image::DynamicImage::from_decoder(decoder)?;
    dynamic.apply_orientation(orientation);
    Ok(dynamic)
}

trait GlyphFactory {
    fn get_glyph(
        &mut self,
        font: &FontItem,
        glyph: u32,
        style: &DrawStyle,
        pos: Vec2,
    ) -> Option<Arc<VecScene>>;

    fn resolve_paint(&self, paint: &ImmutStr) -> PaintBrush;

    fn svg_resource_resolver(&self) -> Option<&dyn SvgResourceResolver>;
}

#[derive(Clone, Debug)]
struct DrawStyle {
    fill: Option<PaintBrush>,
    fill_rule: peniko::Fill,
    stroke: Option<StrokeStyle>,
}

impl Default for DrawStyle {
    fn default() -> Self {
        Self {
            fill: None,
            fill_rule: peniko::Fill::NonZero,
            stroke: None,
        }
    }
}

#[derive(Clone, Debug)]
struct StrokeStyle {
    brush: PaintBrush,
    stroke: kurbo::Stroke,
}

impl DrawStyle {
    fn has_draw(&self) -> bool {
        self.fill.is_some() || self.stroke.is_some()
    }

    fn translated_for_glyph(&self, pos: Vec2) -> Self {
        Self {
            fill: self
                .fill
                .clone()
                .map(|paint| paint.translated_for_glyph(pos)),
            fill_rule: self.fill_rule,
            stroke: self.stroke.as_ref().map(|stroke| StrokeStyle {
                brush: stroke.brush.clone().translated_for_glyph(pos),
                stroke: stroke.stroke.clone(),
            }),
        }
    }
}

fn resolve_draw_style<'m, C>(ctx: &C, styles: &[PathStyle], stroke_scale: f64) -> DrawStyle
where
    C: RenderVm<'m, Resultant = Arc<VecScene>> + GlyphFactory,
{
    let mut draw = DrawStyle::default();
    let mut stroke_brush = PaintBrush::black();
    let mut stroke = false;
    let mut stroke_width = 0f64;
    let mut stroke_join = kurbo::Join::Miter;
    let mut stroke_cap = kurbo::Cap::Butt;
    let mut stroke_miter_limit = 4f64;
    let mut dash_pattern = SmallVec::new();
    let mut dash_offset = 0f64;

    for style in styles {
        match style {
            PathStyle::Fill(color) => {
                draw.fill = Some(ctx.resolve_paint(color));
            }
            PathStyle::Stroke(color) => {
                stroke_brush = ctx.resolve_paint(color);
                stroke = true;
            }
            PathStyle::StrokeWidth(width) => {
                stroke_width = f64::from(width.0) * stroke_scale;
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
                stroke_miter_limit = f64::from(limit.0);
            }
            PathStyle::StrokeDashArray(array) => {
                dash_pattern = array
                    .iter()
                    .map(|dash| f64::from(dash.0) * stroke_scale)
                    .collect();
            }
            PathStyle::StrokeDashOffset(offset) => {
                dash_offset = f64::from(offset.0) * stroke_scale;
            }
            PathStyle::FillRule(rule) => {
                draw.fill_rule = match rule.as_ref() {
                    "nonzero" => peniko::Fill::NonZero,
                    "evenodd" => peniko::Fill::EvenOdd,
                    _ => peniko::Fill::NonZero,
                };
            }
        }
    }

    if stroke {
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

        draw.stroke = Some(StrokeStyle {
            brush: stroke_brush,
            stroke: kurbo_stroke,
        });
    }

    draw
}

fn render_path_with_style<'m, C>(
    ctx: &mut C,
    scene: &mut Scene,
    path: &kurbo::BezPath,
    style: &DrawStyle,
) where
    C: RenderVm<'m, Resultant = Arc<VecScene>> + GlyphFactory,
{
    if let Some(fill) = &style.fill {
        fill_path_with_paint(ctx, scene, style.fill_rule, path, fill);
    }

    if let Some(stroke) = &style.stroke {
        stroke_path_with_paint(ctx, scene, &stroke.stroke, path, &stroke.brush);
    }
}

#[derive(Clone, Debug)]
enum PaintBrush {
    Brush {
        brush: peniko::Brush,
        transform: Option<Affine>,
    },
    Pattern(PatternPaint),
}

#[derive(Clone, Debug)]
struct PatternPaint {
    pattern: Arc<PatternItem>,
    transform: Affine,
}

impl PaintBrush {
    fn black() -> Self {
        Self::solid(peniko::Color::BLACK)
    }

    fn solid(color: peniko::Color) -> Self {
        Self::Brush {
            brush: peniko::Brush::Solid(color),
            transform: None,
        }
    }

    fn translated_for_glyph(self, pos: Vec2) -> Self {
        match self {
            Self::Brush {
                brush,
                mut transform,
            } => {
                if let peniko::Brush::Gradient(_) = brush {
                    let matrix = transform.unwrap_or(Affine::IDENTITY);
                    transform = Some(matrix.then_translate(-pos));
                }

                Self::Brush { brush, transform }
            }
            Self::Pattern(mut pattern) => {
                pattern.transform = pattern.transform.then_translate(-pos);
                Self::Pattern(pattern)
            }
        }
    }
}

fn resolve_paint(module: &Module, paint: &ImmutStr) -> PaintBrush {
    if paint.starts_with("@g") {
        resolve_gradient(module, paint.as_ref()).unwrap_or_else(PaintBrush::black)
    } else if paint.starts_with("@p") {
        resolve_pattern(module, paint.as_ref()).unwrap_or_else(PaintBrush::black)
    } else if paint.starts_with('@') {
        PaintBrush::black()
    } else {
        PaintBrush::solid(
            peniko::color::parse_color(paint.as_ref())
                .map(|it| it.to_alpha_color())
                .unwrap_or(peniko::Color::BLACK),
        )
    }
}

fn fill_path_with_paint<'m, C>(
    ctx: &mut C,
    scene: &mut Scene,
    fill_rule: peniko::Fill,
    path: &kurbo::BezPath,
    paint: &PaintBrush,
) where
    C: RenderVm<'m, Resultant = Arc<VecScene>> + GlyphFactory,
{
    match paint {
        PaintBrush::Brush { brush, transform } => {
            scene.fill(fill_rule, Affine::IDENTITY, brush, *transform, path);
        }
        PaintBrush::Pattern(pattern) => {
            scene.push_clip_layer(fill_rule, Affine::IDENTITY, path);
            render_pattern_tiles(ctx, scene, pattern, path.bounding_box());
            scene.pop_layer();
        }
    }
}

fn stroke_path_with_paint<'m, C>(
    ctx: &mut C,
    scene: &mut Scene,
    stroke: &kurbo::Stroke,
    path: &kurbo::BezPath,
    paint: &PaintBrush,
) where
    C: RenderVm<'m, Resultant = Arc<VecScene>> + GlyphFactory,
{
    match paint {
        PaintBrush::Brush { brush, transform } => {
            scene.stroke(stroke, Affine::IDENTITY, brush, *transform, path);
        }
        PaintBrush::Pattern(pattern) => {
            if stroke.width == 0. {
                return;
            }

            scene.push_clip_layer(stroke, Affine::IDENTITY, path);
            render_pattern_tiles(ctx, scene, pattern, stroke_pattern_bounds(path, stroke));
            scene.pop_layer();
        }
    }
}

fn stroke_pattern_bounds(path: &kurbo::BezPath, stroke: &kurbo::Stroke) -> Rect {
    let padding = stroke.width * stroke.miter_limit.max(1.);
    path.bounding_box().inset(padding)
}

fn render_pattern_tiles<'m, C>(ctx: &mut C, scene: &mut Scene, pattern: &PatternPaint, bounds: Rect)
where
    C: RenderVm<'m, Resultant = Arc<VecScene>> + GlyphFactory,
{
    if !is_valid_rect(bounds) {
        return;
    }

    let tile_width = f64::from(pattern.pattern.size.x.0 + pattern.pattern.spacing.x.0);
    let tile_height = f64::from(pattern.pattern.size.y.0 + pattern.pattern.spacing.y.0);
    if tile_width <= f64::EPSILON || tile_height <= f64::EPSILON {
        return;
    }

    let pattern_bounds = pattern.transform.inverse().transform_rect_bbox(bounds);
    if !is_valid_rect(pattern_bounds) {
        return;
    }

    let tile = ctx.render_item(&pattern.pattern.frame);
    let mut tile_scene = Scene::new();
    let tile_clip = Rect::new(0., 0., tile_width, tile_height);
    tile_scene.push_clip_layer(peniko::Fill::NonZero, Affine::IDENTITY, &tile_clip);
    tile.render(&mut tile_scene);
    tile_scene.pop_layer();

    let start_x = tile_start(pattern_bounds.min_x(), tile_width);
    let end_x = tile_end(pattern_bounds.max_x(), tile_width);
    let start_y = tile_start(pattern_bounds.min_y(), tile_height);
    let end_y = tile_end(pattern_bounds.max_y(), tile_height);

    for y in start_y..=end_y {
        for x in start_x..=end_x {
            let tile_origin = Vec2::new(f64::from(x) * tile_width, f64::from(y) * tile_height);
            scene.append(
                &tile_scene,
                Some(pattern.transform.pre_translate(tile_origin)),
            );
        }
    }
}

fn is_valid_rect(rect: Rect) -> bool {
    rect.x0.is_finite()
        && rect.y0.is_finite()
        && rect.x1.is_finite()
        && rect.y1.is_finite()
        && !rect.is_zero_area()
}

fn tile_start(value: f64, step: f64) -> i32 {
    tile_index((value / step).floor() - 1.)
}

fn tile_end(value: f64, step: f64) -> i32 {
    tile_index((value / step).ceil() + 1.)
}

fn tile_index(value: f64) -> i32 {
    value.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

fn resolve_gradient(module: &Module, paint: &str) -> Option<PaintBrush> {
    let id = paint.strip_prefix("@g")?;
    let mut fingerprint = parse_fingerprint(id)?;
    let mut transform = None;

    if let Some(ir::VecItem::ColorTransform(color_transform)) = module.get_item(&fingerprint) {
        fingerprint = color_transform.item;
        transform = Some(convert_transform(&color_transform.transform));
    }

    let gradient = match module.get_item(&fingerprint) {
        Some(ir::VecItem::Gradient(gradient)) => gradient.as_ref(),
        _ => return None,
    };

    let converted = convert_gradient(gradient)?;
    if let Some(gradient_transform) = converted.transform {
        transform = Some(match transform {
            Some(transform) => transform * gradient_transform,
            None => gradient_transform,
        });
    }

    Some(PaintBrush::Brush {
        brush: peniko::Brush::Gradient(converted.gradient),
        transform,
    })
}

fn resolve_pattern(module: &Module, paint: &str) -> Option<PaintBrush> {
    let id = paint.strip_prefix("@p")?;
    let mut fingerprint = parse_fingerprint(id)?;
    let mut transform = Affine::IDENTITY;

    if let Some(ir::VecItem::ColorTransform(color_transform)) = module.get_item(&fingerprint) {
        fingerprint = color_transform.item;
        transform = convert_transform(&color_transform.transform);
    }

    let pattern = match module.get_item(&fingerprint) {
        Some(ir::VecItem::Pattern(pattern)) => pattern.clone(),
        _ => return None,
    };

    Some(PaintBrush::Pattern(PatternPaint { pattern, transform }))
}

fn parse_fingerprint(id: &str) -> Option<Fingerprint> {
    if id.len() < 11 {
        return None;
    }

    Fingerprint::try_from_str(id).ok()
}

fn convert_transform(m: &ir::Transform) -> Affine {
    Affine::new([
        m.sx.0 as f64,
        m.ky.0 as f64,
        m.kx.0 as f64,
        m.sy.0 as f64,
        m.tx.0 as f64,
        m.ty.0 as f64,
    ])
}

struct ConvertedGradient {
    gradient: peniko::Gradient,
    transform: Option<Affine>,
}

const VELLO_GRADIENT_SAMPLES: usize = 512;

fn convert_gradient(gradient: &GradientItem) -> Option<ConvertedGradient> {
    if gradient.stops.is_empty() {
        return None;
    }

    let mut transform = None;
    let (stops, interpolation_cs) = convert_gradient_stops(gradient);

    let mut peniko_gradient = match &gradient.kind {
        GradientKind::Linear(angle) => {
            let (start, end) = linear_gradient_points(angle.0);
            peniko::Gradient::new_linear(start, end)
        }
        GradientKind::Radial(radius) => {
            let mut center = Axes::new(Scalar(0.5), Scalar(0.5));
            let mut focal_center = Axes::new(Scalar(0.5), Scalar(0.5));
            let mut focal_radius = Scalar(0.);

            for style in &gradient.styles {
                match style {
                    GradientStyle::Center(value) => center = *value,
                    GradientStyle::FocalCenter(value) => focal_center = *value,
                    GradientStyle::FocalRadius(value) => focal_radius = *value,
                }
            }

            peniko::Gradient::new_two_point_radial(
                (focal_center.x.0 as f64, focal_center.y.0 as f64),
                focal_radius.0,
                (center.x.0 as f64, center.y.0 as f64),
                radius.0,
            )
        }
        GradientKind::Conic(angle) => {
            let mut center = Axes::new(Scalar(0.5), Scalar(0.5));
            for style in &gradient.styles {
                if let GradientStyle::Center(value) = style {
                    center = *value;
                }
            }

            transform = Some(conic_gradient_transform(center, *angle));
            peniko::Gradient::new_sweep(
                (center.x.0 as f64, center.y.0 as f64),
                0.,
                std::f32::consts::TAU,
            )
        }
    };

    peniko_gradient.interpolation_cs = interpolation_cs;
    peniko_gradient.stops = stops;

    Some(ConvertedGradient {
        gradient: peniko_gradient,
        transform,
    })
}

fn convert_gradient_stops(
    gradient: &GradientItem,
) -> (peniko::ColorStops, peniko::color::ColorSpaceTag) {
    // Vello 0.7 resolves gradient ramps in sRGB regardless of
    // `Gradient::interpolation_cs`, so pre-sample non-sRGB spaces with Typst's
    // color mixer before handing the stops to Vello.
    if let Some(stops) = sampled_gradient_stops(gradient) {
        return (stops, peniko::color::ColorSpaceTag::Srgb);
    }

    (
        raw_gradient_stops(gradient),
        color_space_tag(gradient.space),
    )
}

fn sampled_gradient_stops(gradient: &GradientItem) -> Option<peniko::ColorStops> {
    let mixing_space = typst_color_space(gradient.space)?;
    if gradient.stops.len() < 2 {
        return None;
    }

    let mut stops = peniko::ColorStops::new();
    for index in 0..VELLO_GRADIENT_SAMPLES {
        let offset = index as f32 / (VELLO_GRADIENT_SAMPLES - 1) as f32;
        stops.push(peniko::ColorStop {
            offset,
            color: dynamic_color_from_typst(sample_gradient_color(
                gradient,
                mixing_space,
                offset as f64,
            )?),
        });
    }

    Some(stops)
}

fn raw_gradient_stops(gradient: &GradientItem) -> peniko::ColorStops {
    let mut stops = peniko::ColorStops::new();
    for (color, offset) in &gradient.stops {
        stops.push(peniko::ColorStop {
            offset: offset.0,
            color: dynamic_color_from_rgba8(*color),
        });
    }

    stops
}

fn sample_gradient_color(
    gradient: &GradientItem,
    mixing_space: TypstColorSpace,
    t: f64,
) -> Option<TypstColor> {
    let t = t.clamp(0.0, 1.0);
    let stops = &gradient.stops;
    let mut index = stops.partition_point(|(_, ratio)| f64::from(ratio.0) < t);

    if index == 0 {
        while stops
            .get(index + 1)
            .is_some_and(|(_, ratio)| ratio.0 == 0.0)
        {
            index += 1;
        }

        return Some(typst_color_from_rgba8(stops[index].0));
    }

    if index >= stops.len() {
        return Some(typst_color_from_rgba8(stops.last()?.0));
    }

    let (col_0, pos_0) = stops[index - 1];
    let (col_1, pos_1) = stops[index];
    let span = pos_1.0 - pos_0.0;
    let t = if span.abs() <= f32::EPSILON {
        0.0
    } else {
        (t - f64::from(pos_0.0)) / f64::from(span)
    };

    TypstColor::mix_iter(
        [
            WeightedColor::new(typst_color_from_rgba8(col_0), 1.0 - t),
            WeightedColor::new(typst_color_from_rgba8(col_1), t),
        ],
        mixing_space,
    )
    .ok()
}

fn typst_color_space(space: ColorSpace) -> Option<TypstColorSpace> {
    Some(match space {
        ColorSpace::Luma | ColorSpace::Srgb => return None,
        ColorSpace::Oklab => TypstColorSpace::Oklab,
        ColorSpace::Oklch => TypstColorSpace::Oklch,
        ColorSpace::D65Gray => TypstColorSpace::D65Gray,
        ColorSpace::LinearRgb => TypstColorSpace::LinearRgb,
        ColorSpace::Hsl => TypstColorSpace::Hsl,
        ColorSpace::Hsv => TypstColorSpace::Hsv,
        ColorSpace::Cmyk => TypstColorSpace::Cmyk,
    })
}

fn typst_color_from_rgba8(color: ir::Rgba8Item) -> TypstColor {
    TypstColor::from_u8(color.r, color.g, color.b, color.a)
}

fn dynamic_color_from_rgba8(color: ir::Rgba8Item) -> peniko::color::DynamicColor {
    peniko::color::DynamicColor::from_alpha_color(peniko::Color::from_rgba8(
        color.r, color.g, color.b, color.a,
    ))
}

fn dynamic_color_from_typst(color: TypstColor) -> peniko::color::DynamicColor {
    let (r, g, b, a) = color.to_rgb().into_format::<u8, u8>().into_components();
    dynamic_color_from_rgba8(ir::Rgba8Item { r, g, b, a })
}

fn conic_gradient_transform(center: Axes<Scalar>, angle: Scalar) -> Affine {
    let center = Vec2::new(center.x.0 as f64, center.y.0 as f64);
    scale_non_uniform_about(-1., 1., center)
        * Affine::rotate_about(-(angle.0 as f64), (center.x, center.y))
}

fn scale_non_uniform_about(scale_x: f64, scale_y: f64, center: Vec2) -> Affine {
    Affine::translate(-center)
        .then_scale_non_uniform(scale_x, scale_y)
        .then_translate(center)
}

fn linear_gradient_points(angle: f32) -> ((f64, f64), (f64, f64)) {
    let angle = angle.rem_euclid(std::f32::consts::TAU);
    let (sin, cos) = angle.sin_cos();
    let length = sin.abs() + cos.abs();

    match angle {
        angle if angle < std::f32::consts::FRAC_PI_2 => {
            ((0., 0.), ((cos * length) as f64, (sin * length) as f64))
        }
        angle if angle < std::f32::consts::PI => (
            (1., 0.),
            ((cos * length + 1.) as f64, (sin * length) as f64),
        ),
        angle if angle < 3. * std::f32::consts::FRAC_PI_2 => (
            (1., 1.),
            ((cos * length + 1.) as f64, (sin * length + 1.) as f64),
        ),
        _ => (
            (0., 1.),
            ((cos * length) as f64, (sin * length + 1.) as f64),
        ),
    }
}

fn color_space_tag(space: ColorSpace) -> peniko::color::ColorSpaceTag {
    match space {
        ColorSpace::Oklab => peniko::color::ColorSpaceTag::Oklab,
        ColorSpace::Oklch => peniko::color::ColorSpaceTag::Oklch,
        ColorSpace::Srgb => peniko::color::ColorSpaceTag::Srgb,
        ColorSpace::LinearRgb => peniko::color::ColorSpaceTag::LinearSrgb,
        ColorSpace::Hsl => peniko::color::ColorSpaceTag::Hsl,
        ColorSpace::Luma | ColorSpace::D65Gray | ColorSpace::Hsv | ColorSpace::Cmyk => {
            peniko::color::ColorSpaceTag::Srgb
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_gradient(kind: GradientKind) -> ir::VecItem {
        ir::VecItem::Gradient(Arc::new(GradientItem {
            stops: vec![
                (
                    ir::Rgba8Item {
                        r: 255,
                        g: 0,
                        b: 0,
                        a: 255,
                    },
                    Scalar(0.),
                ),
                (
                    ir::Rgba8Item {
                        r: 0,
                        g: 0,
                        b: 255,
                        a: 255,
                    },
                    Scalar(1.),
                ),
            ],
            anti_alias: true,
            space: ColorSpace::Oklab,
            kind,
            styles: vec![],
        }))
    }

    fn sample_pattern(frame: Fingerprint) -> ir::VecItem {
        ir::VecItem::Pattern(Arc::new(PatternItem {
            frame,
            size: Axes::new(Scalar(4.), Scalar(5.)),
            spacing: Axes::new(Scalar(1.), Scalar(2.)),
        }))
    }

    #[test]
    fn resolves_solid_paint() {
        let module = Module::default();

        let paint = resolve_paint(&module, &"#f00".into());

        let PaintBrush::Brush { brush, transform } = paint else {
            panic!("expected brush paint");
        };

        assert!(transform.is_none());
        assert_eq!(
            brush,
            peniko::Brush::Solid(peniko::Color::from_rgb8(255, 0, 0))
        );
    }

    #[test]
    fn resolves_direct_linear_gradient_paint() {
        let mut module = Module::default();
        let gradient_id = Fingerprint::from_pair(1, 0);
        module.items.insert(
            gradient_id,
            sample_gradient(GradientKind::Linear(Scalar(0.))),
        );

        let paint = resolve_paint(&module, &format!("@{}", gradient_id.as_svg_id("g")).into());

        let PaintBrush::Brush { brush, transform } = paint else {
            panic!("expected brush paint");
        };

        assert!(transform.is_none());
        let peniko::Brush::Gradient(gradient) = brush else {
            panic!("expected gradient brush");
        };

        assert_eq!(
            gradient.interpolation_cs,
            peniko::color::ColorSpaceTag::Srgb
        );
        assert_eq!(gradient.stops.len(), VELLO_GRADIENT_SAMPLES);
        assert_eq!(gradient.stops[0].offset, 0.);
        assert_eq!(gradient.stops[VELLO_GRADIENT_SAMPLES - 1].offset, 1.);
        assert!(matches!(gradient.kind, peniko::GradientKind::Linear(_)));
    }

    #[test]
    fn resolves_gradient_paint_with_color_transform() {
        let mut module = Module::default();
        let gradient_id = Fingerprint::from_pair(1, 0);
        let transform_id = Fingerprint::from_pair(2, 0);
        let transform = ir::Transform {
            sx: Scalar(2.),
            ky: Scalar(0.),
            kx: Scalar(0.),
            sy: Scalar(3.),
            tx: Scalar(4.),
            ty: Scalar(5.),
        };

        module.items.insert(
            gradient_id,
            sample_gradient(GradientKind::Radial(Scalar(0.75))),
        );
        module.items.insert(
            transform_id,
            ir::VecItem::ColorTransform(Arc::new(ir::ColorTransform {
                transform,
                item: gradient_id,
            })),
        );

        let paint = resolve_paint(&module, &format!("@{}", transform_id.as_svg_id("g")).into());

        let PaintBrush::Brush {
            brush,
            transform: paint_transform,
        } = paint
        else {
            panic!("expected brush paint");
        };

        assert_eq!(paint_transform, Some(convert_transform(&transform)));
        assert!(matches!(brush, peniko::Brush::Gradient(_)));
    }

    #[test]
    fn resolves_conic_gradient_with_typst_orientation() {
        let gradient = sample_gradient(GradientKind::Conic(Scalar(0.)));
        let ir::VecItem::Gradient(gradient) = gradient else {
            panic!("expected gradient item");
        };

        let converted = convert_gradient(&gradient).expect("gradient should convert");
        assert_eq!(
            converted.transform,
            Some(Affine::new([-1., 0., 0., 1., 1., 0.]))
        );

        let peniko_gradient = converted.gradient;
        let peniko::GradientKind::Sweep(sweep) = peniko_gradient.kind else {
            panic!("expected sweep gradient");
        };

        assert_eq!(sweep.start_angle, 0.);
        assert_eq!(sweep.end_angle, std::f32::consts::TAU);
    }

    #[test]
    fn resolves_direct_pattern_paint() {
        let mut module = Module::default();
        let frame_id = Fingerprint::from_pair(1, 0);
        let pattern_id = Fingerprint::from_pair(2, 0);
        module.items.insert(pattern_id, sample_pattern(frame_id));

        let paint = resolve_paint(&module, &format!("@{}", pattern_id.as_svg_id("p")).into());

        let PaintBrush::Pattern(pattern) = paint else {
            panic!("expected pattern paint");
        };

        assert_eq!(pattern.pattern.frame, frame_id);
        assert_eq!(pattern.pattern.size, Axes::new(Scalar(4.), Scalar(5.)));
        assert_eq!(pattern.pattern.spacing, Axes::new(Scalar(1.), Scalar(2.)));
        assert_eq!(pattern.transform, Affine::IDENTITY);
    }

    #[test]
    fn resolves_pattern_paint_with_color_transform() {
        let mut module = Module::default();
        let frame_id = Fingerprint::from_pair(1, 0);
        let pattern_id = Fingerprint::from_pair(2, 0);
        let transform_id = Fingerprint::from_pair(3, 0);
        let transform = ir::Transform {
            sx: Scalar(2.),
            ky: Scalar(0.),
            kx: Scalar(0.),
            sy: Scalar(3.),
            tx: Scalar(4.),
            ty: Scalar(5.),
        };

        module.items.insert(pattern_id, sample_pattern(frame_id));
        module.items.insert(
            transform_id,
            ir::VecItem::ColorTransform(Arc::new(ir::ColorTransform {
                transform,
                item: pattern_id,
            })),
        );

        let paint = resolve_paint(&module, &format!("@{}", transform_id.as_svg_id("p")).into());

        let PaintBrush::Pattern(pattern) = paint else {
            panic!("expected pattern paint");
        };

        assert_eq!(pattern.pattern.frame, frame_id);
        assert_eq!(pattern.transform, convert_transform(&transform));
    }

    #[test]
    fn keeps_unknown_reference_as_black_fallback() {
        let module = Module::default();

        let paint = resolve_paint(&module, &"@pAQAAAAAAAAA".into());

        let PaintBrush::Brush { brush, transform } = paint else {
            panic!("expected brush paint");
        };

        assert!(transform.is_none());
        assert_eq!(brush, peniko::Brush::Solid(peniko::Color::BLACK));
    }
}
