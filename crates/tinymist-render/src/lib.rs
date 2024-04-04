//! # tinymist-render
//!
//! **Note: this crate is under development. it currently doesn't ensure stable
//! APIs, and heavily depending on some unstable crates.**
//!
//! This crate provides rendering features for tinymist server.

use core::fmt;

use base64::Engine;
use tinymist_query::{AnalysisContext, FramePosition, VersionedDocument};
use typst_ts_svg_exporter::{ExportFeature, SvgExporter, SvgText};

struct TelescopeExportFeature {}

impl ExportFeature for TelescopeExportFeature {
    const ENABLE_INLINED_SVG: bool = false;
    const ENABLE_TRACING: bool = false;
    const SHOULD_ATTACH_DEBUG_INFO: bool = false;
    const SHOULD_RENDER_TEXT_ELEMENT: bool = false;
    const USE_STABLE_GLYPH_ID: bool = true;
    const SHOULD_RASTERIZE_TEXT: bool = false;
    const WITH_BUILTIN_CSS: bool = true;
    const WITH_RESPONSIVE_JS: bool = false;
    const AWARE_HTML_ENTITY: bool = false;
}

/// The renderer in telescope mode.
#[derive(Debug, Clone)]
pub struct TelescopeRenderer {
    telescope_y_above: f32,
    telescope_y_below: f32,
    render_scale: f32,
    invert_color: bool,
}

impl Default for TelescopeRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl TelescopeRenderer {
    /// Create a new telescope renderer.
    pub fn new() -> Self {
        Self {
            telescope_y_above: 55.,
            telescope_y_below: 55.,
            render_scale: 1.5,
            invert_color: true,
        }
    }

    /// Render the telescope image for the given document into markdown format.
    pub fn render_marked(
        &self,
        ctx: &mut AnalysisContext,
        doc: VersionedDocument,
        pos: FramePosition,
    ) -> Option<String> {
        let (svg_payload, w, h) = self.render(ctx, doc, pos)?;

        let sw = w * self.render_scale;
        let sh = h * self.render_scale;

        log::info!("telescope image: {sw}x{sh}, {svg_payload}");

        // encode as markdown dataurl image
        let base64 = base64::engine::general_purpose::STANDARD.encode(svg_payload);
        Some(enlarge_image(format_args!(
            "![Telescope Image](data:image/svg+xml;base64,{base64}|width={sw}|height={sh})"
        )))
    }

    /// Render the telescope image for the given document.
    pub fn render(
        &self,
        _ctx: &mut AnalysisContext,
        doc: VersionedDocument,
        pos: FramePosition,
    ) -> Option<(String, f32, f32)> {
        // todo: svg viewer compablity
        type UsingExporter = SvgExporter<TelescopeExportFeature>;
        let mut doc = UsingExporter::svg_doc(&doc.document);
        doc.module.prepare_glyphs();
        let page0 = doc.pages.get(pos.page.get() - 1)?.clone();
        let mut svg_text = UsingExporter::render(&doc.module, &[page0.clone()], None);

        // todo: let typst.ts expose it
        let svg_header = svg_text.get_mut(0)?;

        let y_center = pos.point.y.to_pt() as f32;
        let y_lo = y_center - self.telescope_y_above;
        let y_hi = y_center + self.telescope_y_below;

        let width = page0.size.x.0;
        let height = y_hi - y_lo;

        *svg_header = SvgText::Plain(header_inner(
            page0.size.x.0,
            y_lo,
            y_hi,
            self.render_scale,
            self.invert_color,
        ));

        Some((SvgText::join(svg_text), width, height))
    }
}

fn enlarge_image(md: fmt::Arguments) -> String {
    format!("```\n```\n{md}\n```\n```")
}

/// Render the header of SVG.
/// <svg> .. </svg>
/// ^^^^^
fn header_inner(w: f32, y_lo: f32, y_hi: f32, scale: f32, invert_color: bool) -> String {
    let h = y_hi - y_lo;
    let sw = w * scale;
    let sh = h * scale;

    let invert_style = if invert_color {
        r#"-webkit-filter: invert(0.933333) hue-rotate(180deg); filter: invert(0.933333) hue-rotate(180deg);"#
    } else {
        ""
    };

    format!(
        r#"<svg style="{invert_style}" class="typst-doc" width="{sw:.3}px" height="{sh:.3}px" data-width="{w:.3}" data-height="{h:.3}" viewBox="0 {y_lo:.3} {w:.3} {h:.3}" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:h5="http://www.w3.org/1999/xhtml">"#,
    )
}
