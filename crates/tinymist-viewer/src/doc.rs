// Copyright 2024 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

use std::marker::PhantomData;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ArcStr, ChildrenIds, LayoutCtx, MeasureCtx, MutateCtx, PaintCtx, PropertiesRef,
    RegisterCtx, Widget, WidgetId, WidgetMut,
};
use masonry::layout::{LenReq, Length};
use tracing::{Span, trace_span};
use vello::Scene;
use vello::kurbo::{Axis, Size};
use xilem::core::{Arg, MessageCtx, MessageResult, Mut, View, ViewArgument, ViewMarker};
use xilem::{Pod, ViewCtx};

/// Access a raw vello [`Scene`] within a canvas that fills its parent
pub fn doc<State, F>(draw: F) -> TypstDocPage<State, F>
where
    State: ViewArgument,
    F: Fn(Arg<'_, State>, &mut MutateCtx<'_>, &mut Scene, Size) + Send + Sync + 'static,
{
    TypstDocPage {
        draw,
        alt_text: Option::default(),
        phantom: PhantomData,
    }
}

/// The [`View`] created by [`canvas`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct TypstDocPage<State, F> {
    draw: F,
    alt_text: Option<ArcStr>,
    phantom: PhantomData<fn() -> State>,
}

impl<State, F> TypstDocPage<State, F> {
    /// Sets alt text for the contents of the canvas.
    ///
    /// Users are strongly encouraged to provide alt text for accessibility
    /// tools to use.
    pub fn alt_text(mut self, alt_text: impl Into<ArcStr>) -> Self {
        self.alt_text = Some(alt_text.into());
        self
    }
}

impl<State, F> ViewMarker for TypstDocPage<State, F> {}

impl<State, Action, F> View<State, Action, ViewCtx> for TypstDocPage<State, F>
where
    State: ViewArgument,
    F: Fn(Arg<'_, State>, &mut MutateCtx<'_>, &mut Scene, Size) + Send + Sync + 'static,
{
    type Element = Pod<PageCanvas>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: Arg<'_, State>) -> (Self::Element, Self::ViewState) {
        (
            ctx.with_action_widget(|ctx| {
                let widget = match &self.alt_text {
                    Some(alt_text) => PageCanvas::default().with_alt_text(alt_text.clone()),
                    None => PageCanvas::default(),
                };
                ctx.create_pod(widget)
            }),
            (),
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: Arg<'_, State>,
    ) {
        // , |ctx, scene, size| {
        //     (self.draw)(state, ctx, scene, size);
        // }
        PageCanvas::request_render(&mut element);
        if self.alt_text != prev.alt_text {
            PageCanvas::set_alt_text(&mut element, self.alt_text.clone());
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: Arg<'_, State>,
    ) -> MessageResult<Action> {
        debug_assert!(
            message.remaining_path().is_empty(),
            "id path should be empty in Canvas::message"
        );
        match message.take_message::<CanvasSizeChanged>() {
            Some(_) => MessageResult::RequestRebuild,
            None => {
                log::error!("Wrong message type in Canvas::message, got {message:?}.");
                MessageResult::Stale
            }
        }
    }
}

/// The preferred size of the square Canvas.
const DEFAULT_LENGTH: Length = Length::const_px(100.);

/// A widget used for drawing page.
///
/// A canvas takes a painter callback; every time the canvas is repainted, that
/// callback in run with a [`Scene`].
/// That Scene is then used as the canvas' contents.
#[derive(Default)]
pub struct PageCanvas {
    alt_text: Option<ArcStr>,
    /// The drawable area size, which matches the widget's content-box.
    size: Size,
    scene: Scene,
}

// --- MARK: BUILDERS
impl PageCanvas {
    /// Sets the text that will describe the canvas to screen readers.
    ///
    /// Users are encouraged to set alt text for the canvas.
    /// If possible, the alt-text should succinctly describe what the canvas
    /// represents.
    ///
    /// If the canvas is decorative users should set alt text to `""`.
    /// If it's too hard to describe through text, the alt text should be left
    /// unset. This allows accessibility clients to know that there is no
    /// accessible description of the canvas content.
    pub fn with_alt_text(mut self, alt_text: impl Into<ArcStr>) -> Self {
        self.alt_text = Some(alt_text.into());
        self
    }
}

// --- MARK: METHODS
impl PageCanvas {
    /// Returns the current size of the canvas, which matches its content-box
    /// size.
    pub fn size(&self) -> Size {
        self.size
    }
}

// --- MARK: WIDGETMUT
impl PageCanvas {
    /// Requests a render of the canvas.
    pub fn request_render(this: &mut WidgetMut<'_, Self>) {
        this.ctx.request_render();
    }

    /// Sets the text that will describe the canvas to screen readers.
    ///
    /// See [`Canvas::with_alt_text`] for details.
    pub fn set_alt_text(this: &mut WidgetMut<'_, Self>, alt_text: Option<impl Into<ArcStr>>) {
        this.widget.alt_text = alt_text.map(Into::into);
        this.ctx.request_accessibility_update();
    }
}

/// The size of the canvas has changed.
#[derive(Debug)]
pub struct CanvasSizeChanged {
    /// The new size of the canvas
    pub size: Size,
}

// --- MARK: IMPL WIDGET
impl Widget for PageCanvas {
    type Action = CanvasSizeChanged;

    // TODO - Do we want the Canvas to be transparent to pointer events?
    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        len_req: LenReq,
        _cross_length: Option<f64>,
    ) -> f64 {
        // TODO: Remove HACK: Until scale factor rework happens, just pretend it's
        // always 1.0.       https://github.com/linebender/xilem/issues/1264
        let scale = 1.0;

        // We use all the available space or fall back to our const preferred size.
        match len_req {
            LenReq::FitContent(space) => space,
            _ => DEFAULT_LENGTH.dp(scale),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        if self.size != size {
            self.size = size;
            ctx.submit_action::<Self::Action>(CanvasSizeChanged { size });
        }
        // We clip the contents we draw.
        ctx.set_clip_path(size.to_rect());
    }

    fn paint(&mut self, _: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, scene: &mut Scene) {
        scene.append(&self.scene, None);
    }

    fn accessibility_role(&self) -> Role {
        Role::Canvas
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        if let Some(alt_text) = &self.alt_text {
            node.set_description(&**alt_text);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn make_trace_span(&self, widget_id: WidgetId) -> Span {
        trace_span!("PageCanvas", id = widget_id.trace())
    }

    fn get_debug_text(&self) -> Option<String> {
        self.alt_text.as_ref().map(ToString::to_string)
    }
}
