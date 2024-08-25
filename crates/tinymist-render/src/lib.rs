//! # tinymist-render
//!
//! **Note: this crate is under development. it currently doesn't ensure stable
//! APIs, and heavily depending on some unstable crates.**
//!
//! This crate provides rendering features for tinymist server.

use core::fmt;

use base64::Engine;
use reflexo_vec2svg::{ExportFeature, SvgExporter, SvgText};
use tinymist_query::{AnalysisContext, FramePosition, VersionedDocument};

struct PeriscopeExportFeature {}

impl ExportFeature for PeriscopeExportFeature {
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

/// The arguments for periscope renderer.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriscopeArgs {
    /// The distance above the center line.
    pub y_above: f32,
    /// The distance below the center line.
    pub y_below: f32,
    /// The scale of the image.
    pub scale: f32,
    /// Whether to invert the color. (will become smarter in the future)
    pub invert_color: String,
}

impl Default for PeriscopeArgs {
    fn default() -> Self {
        Self {
            y_above: 55.,
            y_below: 55.,
            scale: 1.5,
            invert_color: "never".to_owned(),
        }
    }
}

/// The renderer in periscope mode.
#[derive(Debug, Clone)]
pub struct PeriscopeRenderer {
    /// The arguments for periscope renderer.
    p: PeriscopeArgs,
}

impl Default for PeriscopeRenderer {
    fn default() -> Self {
        Self::new(PeriscopeArgs::default())
    }
}

impl PeriscopeRenderer {
    /// Create a new periscope renderer.
    pub fn new(args: PeriscopeArgs) -> Self {
        Self { p: args }
    }

    /// Render the periscope image for the given document into markdown format.
    pub fn render_marked(
        &self,
        ctx: &mut AnalysisContext,
        doc: VersionedDocument,
        pos: FramePosition,
    ) -> Option<String> {
        let (svg_payload, w, h) = self.render(ctx, doc, pos)?;

        let sw = w * self.p.scale;
        let sh = h * self.p.scale;

        log::debug!("periscope image: {sw}x{sh}, {svg_payload}");

        // encode as markdown dataurl image
        let base64 = base64::engine::general_purpose::STANDARD.encode(svg_payload);
        Some(enlarge_image(format_args!(
            "![Periscope Mode](data:image/svg+xml;base64,{base64}|width={sw}|height={sh})"
        )))
    }

    /// Render the periscope image for the given document.
    pub fn render(
        &self,
        _ctx: &mut AnalysisContext,
        doc: VersionedDocument,
        pos: FramePosition,
    ) -> Option<(String, f32, f32)> {
        // todo: svg viewer compablity
        type UsingExporter = SvgExporter<PeriscopeExportFeature>;
        let mut doc = UsingExporter::svg_doc(&doc.document);
        doc.module.prepare_glyphs();
        let page0 = doc.pages.get(pos.page.get() - 1)?.clone();
        let mut svg_text = UsingExporter::render(&doc.module, &[page0.clone()], None);

        // todo: let typst.ts expose it
        let svg_header = svg_text.get_mut(0)?;

        let y_center = pos.point.y.to_pt() as f32;
        let y_lo = y_center - self.p.y_above;
        let y_hi = y_center + self.p.y_below;

        let width = page0.size.x.0;
        let height = y_hi - y_lo;

        *svg_header = SvgText::Plain(header_inner(
            page0.size.x.0,
            y_lo,
            y_hi,
            self.p.scale,
            self.p.invert_color == "always",
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
