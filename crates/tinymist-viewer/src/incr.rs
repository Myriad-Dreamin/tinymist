//! Incremental data transfer from backend.

use std::sync::Arc;

use reflexo::{
    error::prelude::*,
    vector::{
        incr::IncrDocClient,
        ir::{ImmutStr, Module, Page},
        vm::RenderVm,
    },
};
use vello::{
    Scene,
    kurbo::{Size, Vec2},
    peniko::{Color, color::parse_color},
};

use crate::VecPage;

/// Incremental pass from vector to vello scene.
pub struct IncrVelloPass {
    /// Fills background color with a css color string
    /// Default is white.
    ///
    /// Note: If the string is empty, the background is transparent.
    pub fill: ImmutStr,
    /// Holds a sequence of vello pages that are rendered
    pub pages: Vec<VecPage>,
}

impl Default for IncrVelloPass {
    fn default() -> Self {
        Self {
            fill: "#ffffff".into(),
            pages: vec![],
        }
    }
}

impl IncrVelloPass {
    /// Interprets the changes in the given module and pages.
    pub fn interpret_changes(&mut self, module: &Module, pages: &[Page]) {
        let mut ct = crate::render::Renderer::new(module);

        let pages: Vec<VecPage> = pages
            .iter()
            .enumerate()
            .map(|(idx, Page { content, size })| {
                if idx < self.pages.len() && self.pages[idx].content_hash == *content {
                    return self.pages[idx].clone();
                }

                let size = Vec2::new(size.x.0 as f64, size.y.0 as f64);
                VecPage {
                    size,
                    elem: ct.render_item(content),
                    content_hash: *content,
                }
            })
            .collect();

        self.pages = pages;
    }

    /// Flushes a page to the canvas with the given transform.
    pub fn flush_page(&mut self, idx: usize) -> (Arc<Scene>, Vec2) {
        if idx >= self.pages.len() {
            log::warn!("Index out of bounds: {idx}");
            return (Arc::new(vello::Scene::new()), Vec2::ZERO);
        }

        let VecPage { size, elem, .. } = &self.pages[idx];

        let mut elem_scene = vello::Scene::new();
        elem.render(&mut elem_scene);

        (Arc::new(elem_scene), *size)
    }
}

/// Maintains the state of the incremental rendering a canvas at client side
#[derive(Default)]
pub struct IncrVelloDocClient {
    /// State of converting vector to canvas
    pub vec2vello: IncrVelloPass,

    /// Expected exact state of the current DOM.
    /// Initially it is None meaning no any page is rendered.
    pub doc_view: Option<Vec<Page>>,
}

impl IncrVelloDocClient {
    /// Resets the state of the incremental rendering.
    pub fn reset(&mut self) {
        let fill = self.vec2vello.fill.clone();
        self.vec2vello = IncrVelloPass {
            fill,
            pages: vec![],
        };
        self.doc_view = None;
    }

    /// Sets canvas's background color
    pub fn set_fill(&mut self, fill: ImmutStr) {
        self.vec2vello.fill = fill;
    }

    /// Returns the configured default page background color.
    pub fn background_color(&self) -> Option<Color> {
        let fill = self.vec2vello.fill.as_ref();
        if fill.is_empty() {
            return None;
        }

        match parse_color(fill) {
            Ok(color) => Some(color.to_alpha_color()),
            Err(err) => {
                log::warn!("Invalid page background color {fill:?}: {err}");
                None
            }
        }
    }

    /// Patches the delta of the incremental rendering.
    fn patch_delta(&mut self, kern: &IncrDocClient) {
        if let Some(layout) = &kern.layout {
            let pages = layout.pages(&kern.doc.module);
            if let Some(pages) = pages {
                self.vec2vello
                    .interpret_changes(pages.module(), pages.pages());
            }
        }
    }

    /// Renders a specific page of the document in the given window.
    pub fn render_pages(&mut self, kern: &mut IncrDocClient) -> Result<Vec<(Arc<Scene>, Size)>> {
        {
            let layouts = kern.doc.layouts[0].by_scalar();
            let Some(layout) = layouts.and_then(|layout| layout.first().cloned()) else {
                return Ok(vec![]);
            };
            kern.set_layout(layout.1.clone());
        }

        self.patch_delta(kern);

        // todo: subpixel: pixel_per_pt
        // let ts = sk::Transform::from_scale(s, s);
        // let ts = Affine::scale(s as f64);

        let res = (0..self.vec2vello.pages.len())
            .map(|idx| {
                let (scene, size) = self.vec2vello.flush_page(idx);
                (scene, Size::new(size.x, size.y))
            })
            .collect();
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use reflexo::vector::incr::IncrDocClient;
    use reflexo::vector::stream::BytesModuleStream;
    use reflexo_vec2svg::IncrSvgDocServer;
    use tinymist_preview::protocol::DIFF_V1_PREFIX;
    use tinymist_std::typst::TypstDocument;
    use vello::Scene;

    use super::{IncrVelloDocClient, IncrVelloPass};

    #[test]
    fn reset_clears_cached_view_state_without_losing_fill() {
        let mut client = IncrVelloDocClient::default();
        client.set_fill("#101010".into());
        client.doc_view = Some(vec![]);

        client.reset();

        assert_eq!(client.vec2vello.fill.as_ref(), "#101010");
        assert!(client.vec2vello.pages.is_empty());
        assert!(client.doc_view.is_none());
    }

    #[test]
    fn renderer_emits_expected_scene_encoding_for_typst_primitives() {
        struct Case {
            name: &'static str,
            source: &'static str,
            check: fn(&Scene),
        }

        let cases = [
            Case {
                name: "filled and stroked path",
                source: r#"
#set page(width: 64pt, height: 64pt, margin: 0pt)
#rect(width: 16pt, height: 16pt, fill: red, stroke: 2pt + blue)
"#,
                check: assert_filled_and_stroked_path_encoding,
            },
            Case {
                name: "transformed clipped group",
                source: r#"
#set page(width: 64pt, height: 64pt, margin: 0pt)
#box(width: 10pt, height: 10pt, clip: true, move(dx: -3pt, dy: -3pt,
  rect(width: 18pt, height: 18pt, fill: green)
))
#rotate(25deg, rect(width: 6pt, height: 6pt, fill: purple))
"#,
                check: assert_transform_and_clip_encoding,
            },
            Case {
                name: "outline text glyphs",
                source: r#"
#set page(width: 64pt, height: 32pt, margin: 0pt)
#text(size: 12pt, fill: black)[Tinymist]
"#,
                check: assert_text_glyph_encoding,
            },
            Case {
                name: "decoded png image",
                source: r#"
#set page(width: 32pt, height: 32pt, margin: 0pt)
#image(bytes((
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
  0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
  0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
  0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
  0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41,
  0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
  0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99,
  0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
  0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
)), format: "png", width: 6pt, height: 6pt)
"#,
                check: assert_image_encoding,
            },
            Case {
                name: "decoded svg image",
                source: r##"
#set page(width: 32pt, height: 32pt, margin: 0pt)
#image(bytes(`<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="#00ff00"/></svg>`.text), format: "svg", width: 6pt, height: 6pt)
"##,
                check: assert_image_encoding,
            },
        ];

        for case in cases {
            let (scene, size) = render_first_page(case.name, case.source);
            assert!(
                size.width > 0.0 && size.height > 0.0,
                "{} fixture should render a non-empty page, got {size:?}",
                case.name
            );
            assert!(
                !scene.encoding().is_empty(),
                "{} fixture should emit vello drawing commands",
                case.name
            );

            (case.check)(&scene);
        }
    }

    #[test]
    fn renderer_reuses_cached_page_when_content_hash_is_unchanged() {
        let mut doc = compile_incremental_doc(
            r#"
#set page(width: 16pt, height: 16pt, margin: 0pt)
#rect(width: 8pt, height: 8pt, fill: black)
"#,
        );
        let layout = select_first_layout(&mut doc);
        let pages = layout
            .pages(&doc.doc.module)
            .expect("renderer fixture should have page metadata");

        let mut pass = IncrVelloPass::default();
        pass.interpret_changes(pages.module(), pages.pages());
        assert_eq!(
            pass.pages.len(),
            1,
            "renderer cache fixture should produce one page"
        );
        let first_elem = Arc::clone(&pass.pages[0].elem);

        pass.interpret_changes(pages.module(), pages.pages());

        assert!(
            Arc::ptr_eq(&first_elem, &pass.pages[0].elem),
            "unchanged page content should keep the cached VecScene allocation"
        );
    }

    fn render_first_page(name: &str, source: &str) -> (Arc<Scene>, vello::kurbo::Size) {
        let mut doc = compile_incremental_doc(source);
        let mut vello = IncrVelloDocClient::default();

        let mut pages = vello
            .render_pages(&mut doc)
            .expect("renderer fixture should render through the vello client");

        assert_eq!(
            pages.len(),
            1,
            "{name} renderer fixture should produce one page"
        );
        pages.pop().expect("one rendered page should exist")
    }

    fn compile_incremental_doc(source: &str) -> IncrDocClient {
        tinymist_tests::run_with_sources(source, |verse, _| {
            let world = verse.snapshot();
            let doc = typst::compile::<typst::layout::PagedDocument>(&world)
                .output
                .expect("short vello renderer fixture should compile");
            let document = TypstDocument::Paged(Arc::new(doc));

            let mut renderer = IncrSvgDocServer::default();
            let frame = renderer.pack_delta(&document);
            assert!(
                frame.starts_with(DIFF_V1_PREFIX),
                "initial renderer fixture frame should be diff-v1"
            );
            let delta =
                BytesModuleStream::from_slice(&frame[DIFF_V1_PREFIX.len()..]).checkout_owned();

            let mut doc = IncrDocClient::default();
            doc.merge_delta(delta);
            doc
        })
    }

    fn select_first_layout(doc: &mut IncrDocClient) -> reflexo::vector::ir::LayoutRegionNode {
        let layout = doc
            .doc
            .layouts
            .first()
            .and_then(|layout| layout.by_scalar())
            .and_then(|layout| layout.first().cloned())
            .expect("renderer fixture should include a scalar layout");
        doc.set_layout(layout.1.clone());
        layout.1
    }

    fn assert_filled_and_stroked_path_encoding(scene: &Scene) {
        let encoding = scene.encoding();
        assert!(
            encoding.n_paths >= 2,
            "filled and stroked shape should encode at least fill and stroke paths, got {}",
            encoding.n_paths
        );
        assert!(
            encoding.draw_tags.len() >= 2,
            "filled and stroked shape should encode at least two draw operations, got {}",
            encoding.draw_tags.len()
        );
        assert!(
            encoding.styles.len() >= 2,
            "filled and stroked shape should encode separate fill and stroke styles, got {}",
            encoding.styles.len()
        );
    }

    fn assert_transform_and_clip_encoding(scene: &Scene) {
        let encoding = scene.encoding();
        assert!(
            encoding.n_clips >= 1,
            "clipped Typst box should encode a vello clip layer"
        );
        assert!(
            encoding.transforms.len() >= 2,
            "moved, rotated, or clipped content should encode multiple transforms, got {}",
            encoding.transforms.len()
        );
    }

    fn assert_text_glyph_encoding(scene: &Scene) {
        let encoding = scene.encoding();
        assert!(
            encoding.n_paths >= 2,
            "outline text should encode glyph outlines as paths, got {}",
            encoding.n_paths
        );
        assert!(
            encoding.draw_tags.len() >= 2,
            "outline text should emit draw operations for glyph outlines, got {}",
            encoding.draw_tags.len()
        );
    }

    fn assert_image_encoding(scene: &Scene) {
        let encoding = scene.encoding();
        assert!(
            encoding.n_paths >= 1,
            "raster image should encode the image bounds as a path"
        );
        assert!(
            !encoding.resources.patches.is_empty(),
            "raster image should attach an image resource patch"
        );
    }
}
