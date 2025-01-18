use crate::hash::Fingerprint;
use std::collections::hash_map::RandomState;
use std::collections::{BTreeMap, HashSet};

use super::ir::{self, Abs, Axes, FontIndice, FontItem, Point, Ratio, Scalar};

/// A build pattern for applying transforms to the group of items.
/// See [`ir::Transform`].
pub trait TransformContext<C>: Sized {
    fn transform_matrix(self, ctx: &mut C, matrix: &ir::Transform) -> Self;
    fn transform_translate(self, ctx: &mut C, xy: Axes<Abs>) -> Self;
    fn transform_scale(self, ctx: &mut C, x: Ratio, y: Ratio) -> Self;
    fn transform_rotate(self, ctx: &mut C, angle: Scalar) -> Self;
    fn transform_skew(self, ctx: &mut C, xy: (Ratio, Ratio)) -> Self;
    fn transform_clip(self, ctx: &mut C, path: &ir::PathItem) -> Self;

    /// See [`ir::TransformItem`].
    fn transform(self, ctx: &mut C, transform: &ir::TransformItem) -> Self {
        use ir::TransformItem::*;
        match transform {
            Matrix(transform) => self.transform_matrix(ctx, transform.as_ref()),
            Translate(xy) => self.transform_translate(ctx, *xy.clone()),
            Scale(xy) => self.transform_scale(ctx, xy.0, xy.1),
            Rotate(angle) => self.transform_rotate(ctx, *angle.clone()),
            Skew(xy) => self.transform_skew(ctx, *xy.clone()),
            Clip(path) => self.transform_clip(ctx, path.as_ref()),
        }
    }
}

/// A RAII trait for rendering vector items into underlying context.
pub trait GroupContext<C>: Sized {
    fn with_frame(self, _ctx: &mut C, _group: &ir::GroupRef) -> Self {
        self
    }
    fn with_text(self, _ctx: &mut C, _text: &ir::TextItem, _fill_key: &Fingerprint) -> Self {
        self
    }
    fn with_label(self, _ctx: &mut C, _label: &str) -> Self {
        self
    }

    fn with_reuse(self, _ctx: &mut C, _v: &Fingerprint) -> Self {
        self
    }

    /// attach shape of the text to the node using css rules.
    fn with_text_shape(
        &mut self,
        _ctx: &mut C,
        _upem: Scalar,
        _shape: &ir::TextShape,
        _fill_key: &Fingerprint,
    ) {
    }

    /// Render a geometrical shape into underlying context.
    fn render_path(&mut self, _ctx: &mut C, _path: &ir::PathItem, _abs_ref: &Fingerprint) {}

    /// Render a semantic link into underlying context.
    fn render_link(&mut self, _ctx: &mut C, _link: &ir::LinkItem) {}

    /// Render an image into underlying context.
    fn render_image(&mut self, _ctx: &mut C, _image_item: &ir::ImageItem) {}

    fn attach_debug_info(&mut self, _ctx: &mut C, _span_id: u64) {}

    /// Render a semantic link into underlying context.
    fn render_content_hint(&mut self, _ctx: &mut C, _ch: char) {}

    /// Render a semantic link into underlying context.
    fn render_html(&mut self, _ctx: &mut C, _html: &ir::HtmlItem) {}

    fn render_item_at(&mut self, ctx: &mut C, pos: Point, item: &Fingerprint);
    fn render_item(&mut self, ctx: &mut C, item: &Fingerprint) {
        self.render_item_at(ctx, Point::default(), item);
    }

    fn render_glyph(&mut self, _ctx: &mut C, _pos: Scalar, _font: &FontItem, _glyph_id: u32) {}

    fn render_text_semantics(&mut self, _ctx: &mut C, _text: &ir::TextItem, _width: Scalar) {}
}

/// A RAII trait for rendering flatten SVG items into underlying context.
pub trait IncrGroupContext<C>: Sized {
    fn render_diff_item_at(
        &mut self,
        ctx: &mut C,

        pos: Point,
        item: &Fingerprint,
        prev_item: &Fingerprint,
    );
    fn render_diff_item(&mut self, ctx: &mut C, item: &Fingerprint, prev_item: &Fingerprint) {
        self.render_diff_item_at(ctx, Point::default(), item, prev_item);
    }
}

/// A virtual machine for rendering a frame.
/// This is a stateful object that is used to render a frame.
/// The 'm lifetime is the lifetime of the module which stores the frame data.
pub trait RenderVm<'m>: Sized + FontIndice<'m> {
    type Resultant;
    type Group: GroupContext<Self> + TransformContext<Self> + Into<Self::Resultant>;

    fn get_item(&self, value: &Fingerprint) -> Option<&'m ir::VecItem>;

    fn start_group(&mut self, value: &Fingerprint) -> Self::Group;

    fn start_frame(&mut self, value: &Fingerprint, _group: &ir::GroupRef) -> Self::Group {
        self.start_group(value)
    }

    // todo: remove render state
    fn start_text(&mut self, value: &Fingerprint, _text: &ir::TextItem) -> Self::Group {
        self.start_group(value)
    }

    #[doc(hidden)]
    /// Default implemenetion to render an item into the a `<g/>` element.
    fn _render_item(&mut self, abs_ref: &Fingerprint) -> Self::Resultant {
        let item: &'m ir::VecItem = self.get_item(abs_ref).unwrap();
        match &item {
            ir::VecItem::Group(group) => self.render_group(abs_ref, group),
            ir::VecItem::Item(transformed) => self.render_transformed_item(abs_ref, transformed),
            ir::VecItem::Labelled(labelled) => self.render_labelled_item(abs_ref, labelled),
            ir::VecItem::Text(text) => {
                let mut g = self.start_text(abs_ref, text);
                g = self.render_text(g, abs_ref, text);

                g.into()
            }
            ir::VecItem::Path(path) => {
                let mut g = self.start_group(abs_ref);
                g.render_path(self, path, abs_ref);
                g.into()
            }
            ir::VecItem::Link(link) => {
                let mut g = self.start_group(abs_ref);
                g.render_link(self, link);
                g.into()
            }
            ir::VecItem::Image(image) => {
                let mut g = self.start_group(abs_ref);
                g.render_image(self, image);
                g.into()
            }
            ir::VecItem::ContentHint(c) => {
                let mut g = self.start_group(abs_ref);
                g.render_content_hint(self, *c);
                g.into()
            }
            ir::VecItem::Html(h) => {
                let mut g = self.start_group(abs_ref);
                g.render_html(self, h);
                g.into()
            }
            ir::VecItem::Color32(..)
            | ir::VecItem::ColorTransform(..)
            | ir::VecItem::Gradient(..)
            | ir::VecItem::Pattern(..)
            | ir::VecItem::None => {
                panic!("FlatRenderVm.RenderFrame.UnknownItem {item:?}")
            }
        }
    }

    /// Render an item into the a `<g/>` element.
    fn render_item(&mut self, abs_ref: &Fingerprint) -> Self::Resultant {
        self._render_item(abs_ref)
    }

    /// Render a frame group into underlying context.
    fn render_group(&mut self, abs_ref: &Fingerprint, group: &ir::GroupRef) -> Self::Resultant {
        let mut group_ctx = self.start_frame(abs_ref, group);

        for (pos, item_ref) in group.0.iter() {
            // let item = self.get_item(&item_ref).unwrap();
            group_ctx.render_item_at(self, *pos, item_ref);
        }

        group_ctx.into()
    }

    /// Render a transformed frame into underlying context.
    fn render_transformed_item(
        &mut self,
        abs_ref: &Fingerprint,
        transformed: &ir::TransformedRef,
    ) -> Self::Resultant {
        let mut ts = self.start_group(abs_ref).transform(self, &transformed.0);

        let item_ref = &transformed.1;
        // let item = self.get_item(&item_ref).unwrap();
        ts.render_item(self, item_ref);
        ts.into()
    }

    /// Render a labelled frame into underlying context.
    fn render_labelled_item(
        &mut self,
        abs_ref: &Fingerprint,
        labelled: &ir::LabelledRef,
    ) -> Self::Resultant {
        let mut ts = self.start_group(abs_ref).with_label(self, &labelled.0);

        let item_ref = &labelled.1;
        // let item = self.get_item(&item_ref).unwrap();
        ts.render_item(self, item_ref);
        ts.into()
    }

    /// Render a text into the underlying context.
    fn render_text(
        &mut self,
        mut group_ctx: Self::Group,
        _abs_ref: &Fingerprint,
        text: &ir::TextItem,
    ) -> Self::Group {
        // upem is the unit per em defined in the font.
        let font = self.get_font(&text.shape.font).unwrap();
        let upem = Scalar(font.units_per_em.0);

        // Rescale the font size and put glyphs into the group.
        group_ctx = text.shape.add_transform(self, group_ctx, upem);
        let mut _width = 0f32;
        for (x, g) in text.render_glyphs(upem, &mut _width) {
            group_ctx.render_glyph(self, x, font, g);
        }

        group_ctx
    }
}

/// A virtual machine that diffs and renders a frame.
/// This is a stateful object that is used to render a frame.
/// The 'm lifetime is the lifetime of the module which stores the frame data.
pub trait IncrRenderVm<'m>: RenderVm<'m> + Sized
where
    Self::Group: IncrGroupContext<Self>,
{
    #[doc(hidden)]
    /// Default implemenetion to Render an item into the a `<g/>` element.
    fn _render_diff_item(
        &mut self,
        next_abs_ref: &Fingerprint,
        prev_abs_ref: &Fingerprint,
    ) -> Self::Resultant {
        let next_item: &'m ir::VecItem = self.get_item(next_abs_ref).unwrap();
        let prev_item = self.get_item(prev_abs_ref);

        let mut group_ctx = self.start_group(next_abs_ref);

        match &next_item {
            ir::VecItem::Group(group) => {
                let mut group_ctx = group_ctx
                    .with_reuse(self, prev_abs_ref)
                    .with_frame(self, group);
                self.render_diff_group(&mut group_ctx, prev_item, group);
                group_ctx
            }
            ir::VecItem::Item(transformed) => {
                let mut group_ctx = group_ctx
                    .with_reuse(self, prev_abs_ref)
                    .transform(self, &transformed.0);
                self.render_diff_transformed_item(&mut group_ctx, prev_item, transformed);
                group_ctx
            }
            ir::VecItem::Labelled(labelled) => {
                let mut group_ctx = group_ctx
                    .with_reuse(self, prev_abs_ref)
                    .with_label(self, &labelled.0);
                self.render_diff_labelled_item(&mut group_ctx, prev_item, labelled);
                group_ctx
            }
            ir::VecItem::Text(text) => {
                let group_ctx = group_ctx.with_text(self, text, next_abs_ref);
                self.render_diff_text(group_ctx, next_abs_ref, prev_abs_ref, text)
            }
            ir::VecItem::Path(path) => {
                group_ctx.render_path(self, path, next_abs_ref);
                group_ctx
            }
            ir::VecItem::Link(link) => {
                group_ctx.render_link(self, link);
                group_ctx
            }
            ir::VecItem::Image(image) => {
                group_ctx.render_image(self, image);
                group_ctx
            }
            ir::VecItem::ContentHint(c) => {
                group_ctx.render_content_hint(self, *c);
                group_ctx
            }
            ir::VecItem::Html(h) => {
                group_ctx.render_html(self, h);
                group_ctx
            }
            ir::VecItem::Color32(..)
            | ir::VecItem::ColorTransform(..)
            | ir::VecItem::Gradient(..)
            | ir::VecItem::Pattern(..)
            | ir::VecItem::None => {
                panic!("FlatRenderVm.RenderFrame.UnknownItem {next_item:?}")
            }
        }
        .into()
    }

    /// Render an item into the a `<g/>` element.
    fn render_diff_item(
        &mut self,
        next_abs_ref: &Fingerprint,
        prev_abs_ref: &Fingerprint,
    ) -> Self::Resultant {
        self._render_diff_item(next_abs_ref, prev_abs_ref)
    }

    /// Render a frame group into underlying context.
    fn render_diff_group(
        &mut self,
        group_ctx: &mut Self::Group,
        prev_item_: Option<&ir::VecItem>,
        next: &ir::GroupRef,
    ) {
        if let Some(ir::VecItem::Group(prev_group)) = prev_item_ {
            let mut unused_prev: BTreeMap<usize, Fingerprint> =
                prev_group.0.iter().map(|v| v.1).enumerate().collect();
            let reusable: HashSet<Fingerprint, RandomState> =
                HashSet::from_iter(prev_group.0.iter().map(|e| e.1));

            for (_, item_ref) in next.0.iter() {
                if reusable.contains(item_ref) {
                    let remove_key = unused_prev.iter().find(|(_, v)| *v == item_ref);
                    if remove_key.is_none() {
                        continue;
                    }
                    unused_prev.remove(&remove_key.unwrap().0.clone());
                }
            }

            for (pos, item_ref) in next.0.iter() {
                if reusable.contains(item_ref) {
                    group_ctx.render_diff_item_at(self, *pos, item_ref, item_ref);
                } else if let Some((_, prev_item_re_)) = &unused_prev.pop_first() {
                    group_ctx.render_diff_item_at(self, *pos, item_ref, prev_item_re_)
                } else {
                    group_ctx.render_item_at(self, *pos, item_ref)
                }
            }
        } else {
            for (pos, item_ref) in next.0.iter() {
                group_ctx.render_item_at(self, *pos, item_ref);
            }
        }
    }

    /// Render a transformed frame into underlying context.
    fn render_diff_transformed_item(
        &mut self,
        ts: &mut Self::Group,
        prev_item_: Option<&ir::VecItem>,
        transformed: &ir::TransformedRef,
    ) {
        let child_ref = &transformed.1;
        match prev_item_ {
            // if both items are transformed, we can reuse the internal item with transforming it a
            // bit.
            Some(ir::VecItem::Item(ir::TransformedRef(_item, prev_ref))) => {
                ts.render_diff_item_at(self, Point::default(), child_ref, prev_ref);
            }
            _ => ts.render_item(self, child_ref),
        }
        // failed to reuse
    }

    /// Render a labelled frame into underlying context.
    fn render_diff_labelled_item(
        &mut self,
        ts: &mut Self::Group,
        prev_item_: Option<&ir::VecItem>,
        labelled: &ir::LabelledRef,
    ) {
        let child_ref = &labelled.1;
        match prev_item_ {
            // if both items are labelled, we can reuse the internal item with transforming it a
            // bit.
            Some(ir::VecItem::Labelled(ir::LabelledRef(_item, prev_ref))) => {
                ts.render_diff_item_at(self, Point::default(), child_ref, prev_ref);
            }
            _ => ts.render_item(self, child_ref),
        }
        // failed to reuse
    }

    /// Render a diff text into the underlying context.
    fn render_diff_text(
        &mut self,
        group_ctx: Self::Group,
        next_abs_ref: &Fingerprint,
        _prev_abs_ref: &Fingerprint,
        text: &ir::TextItem,
    ) -> Self::Group {
        self.render_text(group_ctx, next_abs_ref, text)
    }
}
