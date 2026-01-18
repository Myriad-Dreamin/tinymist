use reflexo::{
    error::prelude::*,
    vector::{
        incr::IncrDocClient,
        ir::{ImmutStr, Module, Page, Rect},
        vm::RenderVm,
    },
};
use vello::kurbo::{Affine, Circle, Ellipse, Line, RoundedRect, Stroke, Vec2};
use vello::peniko::Color;

use crate::{VecPage, VecScene};
// use crate::{set_transform, CanvasDevice, CanvasOp, CanvasPage, CanvasTask,
// DefaultExportFeature};

/// Incremental pass from vector to canvas
pub struct IncrVec2VelloPass {
    /// Canvas's pixel per point
    pub pixel_per_pt: f32,
    /// Fills background color with a css color string
    /// Default is white.
    ///
    /// Note: If the string is empty, the background is transparent.
    pub fill: ImmutStr,
    /// Holds a sequence of canvas pages that are rendered
    pub pages: Vec<VecPage>,
}

impl Default for IncrVec2VelloPass {
    fn default() -> Self {
        Self {
            pixel_per_pt: 2.,
            fill: "#ffffff".into(),
            pages: vec![],
        }
    }
}

impl IncrVec2VelloPass {
    /// Interprets the changes in the given module and pages.
    pub fn interpret_changes(&mut self, module: &Module, pages: &[Page]) {
        // let mut t = CanvasTask::<DefaultExportFeature>::default();

        // let mut ct = t.fork_canvas_render_task(module);

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
    pub fn flush_page(&mut self, scene: &mut vello::Scene, idx: usize, ts: Affine) {
        if idx >= self.pages.len() {
            log::warn!("Index out of bounds: {}", idx);
            return;
        }

        let VecPage { size, elem, .. } = &self.pages[idx];

        let mut elem_scene = vello::Scene::new();
        elem.render(&mut elem_scene);
        scene.append(&elem_scene, Some(ts));

        // , canvas: &dyn CanvasDevice, ts: sk::Transform
        // let pg = &self.pages[idx];

        // if !set_transform(canvas, ts) {
        //     return;
        // }
        // canvas.set_fill_style_str(self.fill.as_ref());
        // canvas.fill_rect(0., 0., pg.size.x.0 as f64, pg.size.y.0 as f64);

        // pg.elem.realize(ts, canvas).await;
    }
}

/// Maintains the state of the incremental rendering a canvas at client side
#[derive(Default)]
pub struct IncrVelloDocClient {
    /// State of converting vector to canvas
    pub vec2vello: IncrVec2VelloPass,

    /// Expected exact state of the current DOM.
    /// Initially it is None meaning no any page is rendered.
    pub doc_view: Option<Vec<Page>>,
}

impl IncrVelloDocClient {
    /// Reset the state of the incremental rendering.
    pub fn reset(&mut self) {}

    /// Set canvas's pixel per point
    pub fn set_pixel_per_pt(&mut self, pixel_per_pt: f32) {
        self.vec2vello.pixel_per_pt = pixel_per_pt;
    }

    /// Set canvas's background color
    pub fn set_fill(&mut self, fill: ImmutStr) {
        self.vec2vello.fill = fill;
    }

    fn patch_delta(&mut self, kern: &IncrDocClient) {
        if let Some(layout) = &kern.layout {
            let pages = layout.pages(&kern.doc.module);
            if let Some(pages) = pages {
                self.vec2vello
                    .interpret_changes(pages.module(), pages.pages());
            }
        }
    }

    /// Render a specific page of the document in the given window.
    pub fn render_pages(
        &mut self,
        kern: &mut IncrDocClient,
        // canvas: &dyn CanvasDevice,
    ) -> Result<vello::Scene> {
        {
            let layouts = kern.doc.layouts[0].by_scalar();
            let Some(layouts) = layouts else {
                return Ok(vello::Scene::new());
            };
            let layout = layouts.first().unwrap();
            let layout = layout.clone();
            kern.set_layout(layout.1.clone());
        }

        self.patch_delta(kern);

        let mut scene = vello::Scene::new();
        let s = self.vec2vello.pixel_per_pt;
        // let ts = sk::Transform::from_scale(s, s);
        let ts = Affine::scale(s as f64);
        self.vec2vello.flush_page(&mut scene, 0, ts);

        Ok(scene)
    }
}
