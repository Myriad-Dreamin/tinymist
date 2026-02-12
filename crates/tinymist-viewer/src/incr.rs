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
    pub fn reset(&mut self) {}

    /// Sets canvas's background color
    pub fn set_fill(&mut self, fill: ImmutStr) {
        self.vec2vello.fill = fill;
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
            let Some(layouts) = layouts else {
                return Ok(vec![]);
            };
            let layout = layouts.first().unwrap();
            let layout = layout.clone();
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
