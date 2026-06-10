//! A scroll portal that owns viewer-local wheel zoom behavior.

use std::any::type_name;
use std::marker::PhantomData;
use std::ops::Range;

use masonry::accesskit::{self, Node, Role};
use masonry::core::keyboard::{Key, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, ComposeCtx, EventCtx, FromDynWidget, LayoutCtx,
    MeasureCtx, Modifiers, NewWidget, PaintCtx, PointerEvent, PointerScrollEvent, PropertiesMut,
    PropertiesRef, RegisterCtx, ScrollDelta, TextEvent, Update, UpdateCtx, Widget, WidgetId,
    WidgetMut, WidgetPod,
};
use masonry::dpi::PhysicalPosition;
use masonry::kurbo::{Affine, Axis, Point, Rect, Size, Stroke, Vec2};
use masonry::layout::{LayoutSize, LenDef, LenReq, SizeDef};
use masonry::theme;
use tracing::{Span, trace_span};
use vello::Scene;
use vello::peniko::Fill;
use xilem::core::{
    Arg, MessageCtx, MessageResult, Mut, View, ViewArgument, ViewId, ViewMarker, ViewPathTracker,
};
use xilem::{Pod, ViewCtx, WidgetView};

use crate::doc::ZoomAction;

const ZOOM_PORTAL_CONTENT_VIEW_ID: ViewId = ViewId::new(0x7a6f6f6d);

const NORMAL_SCROLL_LINE_PX: f64 = 120.0;
const ZOOM_WHEEL_LINE_PX: f64 = 20.0;
const ZOOM_WHEEL_THRESHOLD: f64 = 20.0;
const VIEWPORT_EPSILON: f64 = 1e-12;

/// A view which puts `child` into a scrollable region and handles modified-wheel zoom.
pub fn zoom_portal<Child, State, Action, F>(
    child: Child,
    zoom_scale: f64,
    on_zoom: F,
) -> ZoomPortal<Child, State, Action, F>
where
    State: ViewArgument,
    Child: WidgetView<State, Action>,
    F: Fn(Arg<'_, State>, ZoomAction) -> Action + 'static,
{
    ZoomPortal {
        child,
        zoom_scale,
        on_zoom,
        phantom: PhantomData,
    }
}

/// The [`View`] created by [`zoom_portal`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ZoomPortal<Child, State, Action, F> {
    child: Child,
    zoom_scale: f64,
    on_zoom: F,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<Child, State, Action, F> ViewMarker for ZoomPortal<Child, State, Action, F> {}

impl<Child, State, Action, F> View<State, Action, ViewCtx> for ZoomPortal<Child, State, Action, F>
where
    Child: WidgetView<State, Action>,
    State: ViewArgument,
    Action: 'static,
    F: Fn(Arg<'_, State>, ZoomAction) -> Action + 'static,
{
    type Element = Pod<ZoomPortalWidget<Child::Widget>>;
    type ViewState = Child::ViewState;

    fn build(
        &self,
        ctx: &mut ViewCtx,
        app_state: Arg<'_, State>,
    ) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = ctx.with_id(ZOOM_PORTAL_CONTENT_VIEW_ID, |ctx| {
            self.child.build(ctx, app_state)
        });
        let pod = ctx.with_action_widget(|ctx| {
            ctx.create_pod(ZoomPortalWidget::new(child.new_widget, self.zoom_scale))
        });

        (pod, child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: Arg<'_, State>,
    ) {
        ctx.with_id(ZOOM_PORTAL_CONTENT_VIEW_ID, |ctx| {
            self.child.rebuild(
                &prev.child,
                view_state,
                ctx,
                ZoomPortalWidget::child_mut(&mut element),
                app_state,
            );
        });
        ZoomPortalWidget::set_zoom_scale(&mut element, self.zoom_scale);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(ZOOM_PORTAL_CONTENT_VIEW_ID, |ctx| {
            self.child
                .teardown(view_state, ctx, ZoomPortalWidget::child_mut(&mut element));
        });
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: Arg<'_, State>,
    ) -> MessageResult<Action> {
        match message.take_first() {
            Some(ZOOM_PORTAL_CONTENT_VIEW_ID) => self.child.message(
                view_state,
                message,
                ZoomPortalWidget::child_mut(&mut element),
                app_state,
            ),
            None => match message.take_message::<ZoomPortalAction>() {
                Some(action) => MessageResult::Action((self.on_zoom)(app_state, action.zoom)),
                None => {
                    tracing::error!(
                        "Wrong message type in ZoomPortal::message: {message:?} expected {}",
                        type_name::<ZoomPortalAction>()
                    );
                    MessageResult::Stale
                }
            },
            _ => MessageResult::Stale,
        }
    }
}

/// An action emitted when modified wheel input requests a viewer zoom change.
#[derive(Debug)]
pub struct ZoomPortalAction {
    zoom: ZoomAction,
}

/// A scrolling container with viewer-local modified-wheel zoom.
pub struct ZoomPortalWidget<W: Widget + ?Sized> {
    child: WidgetPod<W>,
    content_size: Size,
    viewport_pos: Point,
    child_origin: Point,
    zoom_scale: f64,
    wheel_delta: f64,
    pending_anchor: Option<ZoomAnchor>,
    scrollbar_drag: Option<ScrollbarDrag>,
}

#[derive(Clone, Copy, Debug)]
struct ZoomAnchor {
    content_pos: Point,
    cursor_pos: Point,
    old_zoom_scale: f64,
}

#[derive(Clone, Copy, Debug)]
struct ScrollbarDrag {
    axis: Axis,
    grab_anchor: f64,
}

#[derive(Clone, Copy, Debug)]
struct ScrollbarGeometry {
    axis: Axis,
    track_rect: Rect,
    cursor_length: f64,
    empty_space_length: f64,
    scroll_range: f64,
}

#[derive(Clone, Copy, Debug)]
enum ScrollChange {
    Delta(Vec2),
    Position(Point),
}

impl<W: Widget + ?Sized> ZoomPortalWidget<W> {
    /// Creates a scrolling container with the given child widget.
    pub fn new(child: NewWidget<W>, zoom_scale: f64) -> Self {
        Self {
            child: child.to_pod(),
            content_size: Size::ZERO,
            viewport_pos: Point::ORIGIN,
            child_origin: Point::ORIGIN,
            zoom_scale: sanitize_scale(zoom_scale),
            wheel_delta: 0.0,
            pending_anchor: None,
            scrollbar_drag: None,
        }
    }

    fn set_viewport_pos_raw(&mut self, portal_size: Size, pos: Point) -> bool {
        let viewport_max_pos = (self.content_size - portal_size).max(Size::ZERO);
        let pos = Point::new(
            pos.x.clamp(0.0, viewport_max_pos.width),
            pos.y.clamp(0.0, viewport_max_pos.height),
        );

        if (pos - self.viewport_pos).hypot2() > VIEWPORT_EPSILON {
            self.viewport_pos = pos;
            true
        } else {
            false
        }
    }

    fn set_viewport_pos_event_ctx(
        &mut self,
        ctx: &mut EventCtx<'_>,
        portal_size: Size,
        pos: Point,
    ) -> bool {
        let changed = self.set_viewport_pos_raw(portal_size, pos);
        if changed {
            ctx.request_compose();
            ctx.request_render();
        }
        changed
    }

    fn set_viewport_axis_progress_event_ctx(
        &mut self,
        ctx: &mut EventCtx<'_>,
        portal_size: Size,
        axis: Axis,
        progress: f64,
    ) {
        let pos = viewport_pos_for_axis_progress(
            self.viewport_pos,
            portal_size,
            self.content_size,
            axis,
            progress,
        );
        self.set_viewport_pos_event_ctx(ctx, portal_size, pos);
    }

    fn apply_scroll_change_event_ctx(
        &mut self,
        ctx: &mut EventCtx<'_>,
        portal_size: Size,
        change: ScrollChange,
    ) -> bool {
        match change {
            ScrollChange::Delta(delta) if delta.x != 0.0 || delta.y != 0.0 => {
                self.set_viewport_pos_event_ctx(ctx, portal_size, self.viewport_pos + delta)
            }
            ScrollChange::Delta(_) => false,
            ScrollChange::Position(pos) => self.set_viewport_pos_event_ctx(ctx, portal_size, pos),
        }
    }

    fn start_scrollbar_drag_event_ctx(
        &mut self,
        ctx: &mut EventCtx<'_>,
        portal_size: Size,
        cursor_pos: Point,
    ) -> bool {
        let Some(scrollbar) = scrollbar_at_pos(portal_size, self.content_size, cursor_pos) else {
            return false;
        };
        let thumb_rect = scrollbar.thumb_rect(self.viewport_pos);

        let mut grab_anchor = 0.5;
        if thumb_rect.contains(cursor_pos) {
            let (start, end) = thumb_rect.get_coords(scrollbar.axis);
            let cursor_major = cursor_pos.get_coord(scrollbar.axis);
            grab_anchor = ((cursor_major - start) / (end - start)).clamp(0.0, 1.0);
        } else {
            let progress = scrollbar.progress_from_mouse(cursor_pos, grab_anchor);
            self.set_viewport_axis_progress_event_ctx(ctx, portal_size, scrollbar.axis, progress);
        }

        self.scrollbar_drag = Some(ScrollbarDrag {
            axis: scrollbar.axis,
            grab_anchor,
        });
        ctx.capture_pointer();
        ctx.set_handled();
        ctx.request_render();
        true
    }

    fn update_scrollbar_drag_event_ctx(
        &mut self,
        ctx: &mut EventCtx<'_>,
        portal_size: Size,
        cursor_pos: Point,
    ) -> bool {
        let Some(ScrollbarDrag { axis, grab_anchor }) = self.scrollbar_drag else {
            return false;
        };

        if let Some(scrollbar) = ScrollbarGeometry::new(axis, portal_size, self.content_size) {
            let progress = scrollbar.progress_from_mouse(cursor_pos, grab_anchor);
            self.set_viewport_axis_progress_event_ctx(ctx, portal_size, axis, progress);
        }
        ctx.set_handled();
        true
    }

    fn pan_viewport_to_raw(&mut self, portal_size: Size, target: Rect) -> bool {
        let viewport = Rect::from_origin_size(self.viewport_pos, portal_size);

        let new_pos_x = compute_pan_position(
            viewport.min_x()..viewport.max_x(),
            target.min_x()..target.max_x(),
        );
        let new_pos_y = compute_pan_position(
            viewport.min_y()..viewport.max_y(),
            target.min_y()..target.max_y(),
        );

        self.set_viewport_pos_raw(portal_size, Point::new(new_pos_x, new_pos_y))
    }
}

impl<W: Widget + FromDynWidget + ?Sized> ZoomPortalWidget<W> {
    /// Returns mutable reference to the child widget.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, W> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    /// Updates the current viewer zoom multiplier.
    pub fn set_zoom_scale(this: &mut WidgetMut<'_, Self>, zoom_scale: f64) {
        let zoom_scale = sanitize_scale(zoom_scale);
        if (this.widget.zoom_scale - zoom_scale).abs() > VIEWPORT_EPSILON {
            this.widget.zoom_scale = zoom_scale;
            this.ctx.request_layout();
        } else if this.widget.pending_anchor.is_some() {
            this.widget.pending_anchor = None;
        }
    }
}

impl<W: Widget + FromDynWidget + ?Sized> Widget for ZoomPortalWidget<W> {
    type Action = ZoomPortalAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        let portal_size = ctx.content_box_size();

        match *event {
            PointerEvent::Down(ref event) => {
                let cursor_pos = ctx.local_position(event.state.position);
                self.start_scrollbar_drag_event_ctx(ctx, portal_size, cursor_pos);
            }
            PointerEvent::Move(ref event) if ctx.is_active() && self.scrollbar_drag.is_some() => {
                let cursor_pos = ctx.local_position(event.current.position);
                self.update_scrollbar_drag_event_ctx(ctx, portal_size, cursor_pos);
            }
            PointerEvent::Scroll(PointerScrollEvent {
                delta, ref state, ..
            }) => {
                if is_zoom_modifier(state.modifiers) {
                    let delta_y = scroll_delta_logical_px(
                        delta,
                        ZOOM_WHEEL_LINE_PX,
                        portal_size,
                        ctx.get_scale_factor(),
                    )
                    .y;

                    if let Some(zoom) = zoom_action_from_wheel_delta(&mut self.wheel_delta, delta_y)
                    {
                        let cursor_pos = ctx.local_position(state.position);
                        self.pending_anchor = Some(ZoomAnchor {
                            content_pos: content_pos_under_cursor(
                                self.viewport_pos,
                                cursor_pos,
                                self.child_origin,
                            ),
                            cursor_pos,
                            old_zoom_scale: self.zoom_scale,
                        });
                        ctx.submit_action::<Self::Action>(ZoomPortalAction { zoom });
                    }
                    ctx.set_handled();
                } else {
                    let delta = -scroll_delta_logical_px(
                        delta,
                        NORMAL_SCROLL_LINE_PX,
                        portal_size,
                        ctx.get_scale_factor(),
                    );

                    if self.apply_scroll_change_event_ctx(
                        ctx,
                        portal_size,
                        ScrollChange::Delta(delta),
                    ) {
                        ctx.set_handled();
                    }
                }
            }
            PointerEvent::Up(..) | PointerEvent::Cancel(..)
                if self.scrollbar_drag.take().is_some() =>
            {
                ctx.set_handled();
                ctx.request_render();
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let portal_size = ctx.content_box_size();

        if let TextEvent::Keyboard(event) = event
            && event.state.is_down()
            && let Some(scroll) = keyboard_scroll_change(&event.key, portal_size, self.content_size)
            && self.apply_scroll_change_event_ctx(ctx, portal_size, scroll)
        {
            ctx.set_handled();
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        let portal_size = ctx.content_box_size();

        if let Some(scroll) = access_scroll_change(event, portal_size)
            && self.apply_scroll_change_event_ctx(ctx, portal_size, scroll)
        {
            ctx.set_handled();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if let Update::RequestPanToChild(target) = event {
            let portal_size = ctx.content_box_size();

            if self.pan_viewport_to_raw(portal_size, *target) {
                ctx.request_compose();
                ctx.request_render();
            }
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<f64>,
    ) -> f64 {
        match len_req {
            LenReq::MinContent => 0.0,
            LenReq::MaxContent => {
                let context_size = LayoutSize::maybe(axis.cross(), cross_length);
                ctx.compute_length(&mut self.child, len_req.into(), context_size, axis, None)
            }
            LenReq::FitContent(space) => space,
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let auto_size = SizeDef::new(LenDef::MaxContent, LenDef::MaxContent);
        let content_size = ctx.compute_size(&mut self.child, auto_size, size.into());
        ctx.run_layout(&mut self.child, content_size);
        self.content_size = content_size;
        self.child_origin = centered_child_origin(size, content_size);

        self.set_viewport_pos_raw(size, self.viewport_pos);

        if let Some(anchor) = self.pending_anchor
            && (self.zoom_scale - anchor.old_zoom_scale).abs() > VIEWPORT_EPSILON
        {
            self.pending_anchor = None;
            let viewport_pos =
                anchored_viewport_after_zoom(anchor, self.zoom_scale, self.child_origin);
            self.set_viewport_pos_raw(size, viewport_pos);
        }
        ctx.set_clip_path(size.to_rect());
        ctx.place_child(&mut self.child, self.child_origin);
    }

    fn compose(&mut self, ctx: &mut ComposeCtx<'_>) {
        ctx.set_child_scroll_translation(
            &mut self.child,
            Vec2::new(-self.viewport_pos.x, -self.viewport_pos.y),
        );
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _scene: &mut Scene) {}

    fn post_paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        scene: &mut Scene,
    ) {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            if let Some(scrollbar) =
                ScrollbarGeometry::new(axis, ctx.content_box_size(), self.content_size)
            {
                paint_scrollbar_thumb(scene, scrollbar.thumb_rect(self.viewport_pos));
            }
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::ScrollView
    }

    fn accessibility(
        &mut self,
        ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_clips_children();

        let portal_size = ctx.content_box_size();
        let content_size = self.content_size;
        let scroll_range = (content_size - portal_size).max(Size::ZERO);

        let can_scroll_x = scroll_range.width > VIEWPORT_EPSILON;
        let can_scroll_y = scroll_range.height > VIEWPORT_EPSILON;

        if can_scroll_x {
            node.set_scroll_x_min(0.0);
            node.set_scroll_x_max(scroll_range.width);
            node.set_scroll_x(self.viewport_pos.x);
            if self.viewport_pos.x > VIEWPORT_EPSILON {
                node.add_action(accesskit::Action::ScrollLeft);
            }
            if self.viewport_pos.x + VIEWPORT_EPSILON < scroll_range.width {
                node.add_action(accesskit::Action::ScrollRight);
            }
        }

        if can_scroll_y {
            node.set_scroll_y_min(0.0);
            node.set_scroll_y_max(scroll_range.height);
            node.set_scroll_y(self.viewport_pos.y);
            if self.viewport_pos.y > VIEWPORT_EPSILON {
                node.add_action(accesskit::Action::ScrollUp);
            }
            if self.viewport_pos.y + VIEWPORT_EPSILON < scroll_range.height {
                node.add_action(accesskit::Action::ScrollDown);
            }
        }

        if can_scroll_y && !can_scroll_x {
            node.set_orientation(accesskit::Orientation::Vertical);
        } else if can_scroll_x && !can_scroll_y {
            node.set_orientation(accesskit::Orientation::Horizontal);
        }

        node.add_child_action(accesskit::Action::ScrollIntoView);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }

    fn make_trace_span(&self, widget_id: WidgetId) -> Span {
        trace_span!("ZoomPortal", id = widget_id.trace())
    }

    fn accepts_focus(&self) -> bool {
        true
    }
}

fn compute_pan_position(viewport: Range<f64>, target: Range<f64>) -> f64 {
    if target.start <= viewport.start && viewport.end <= target.end {
        return viewport.start;
    }
    if viewport.start <= target.start && target.end <= viewport.end {
        return viewport.start;
    }

    let target_width = f64::min(viewport.end - viewport.start, target.end - target.start);
    let viewport_width = viewport.end - viewport.start;

    if viewport.start >= target.start {
        target.end - target_width
    } else {
        target.start + target_width - viewport_width
    }
}

fn keyboard_scroll_change(
    key: &Key,
    portal_size: Size,
    content_size: Size,
) -> Option<ScrollChange> {
    let line = NORMAL_SCROLL_LINE_PX;
    let page_y = portal_size.height;

    match key {
        Key::Named(NamedKey::PageDown) => Some(ScrollChange::Delta(Vec2::new(0.0, page_y))),
        Key::Named(NamedKey::PageUp) => Some(ScrollChange::Delta(Vec2::new(0.0, -page_y))),
        Key::Named(NamedKey::ArrowDown) => Some(ScrollChange::Delta(Vec2::new(0.0, line))),
        Key::Named(NamedKey::ArrowUp) => Some(ScrollChange::Delta(Vec2::new(0.0, -line))),
        Key::Named(NamedKey::ArrowRight) => Some(ScrollChange::Delta(Vec2::new(line, 0.0))),
        Key::Named(NamedKey::ArrowLeft) => Some(ScrollChange::Delta(Vec2::new(-line, 0.0))),
        Key::Named(NamedKey::Home) => Some(ScrollChange::Position(Point::ORIGIN)),
        Key::Named(NamedKey::End) => {
            let scroll_range = (content_size - portal_size).max(Size::ZERO);
            Some(ScrollChange::Position(Point::new(
                scroll_range.width,
                scroll_range.height,
            )))
        }
        _ => None,
    }
}

fn access_scroll_change(event: &AccessEvent, portal_size: Size) -> Option<ScrollChange> {
    let unit = if let Some(accesskit::ActionData::ScrollUnit(unit)) = &event.data {
        *unit
    } else {
        accesskit::ScrollUnit::Item
    };
    let amount = match unit {
        accesskit::ScrollUnit::Item => NORMAL_SCROLL_LINE_PX,
        accesskit::ScrollUnit::Page => match event.action {
            accesskit::Action::ScrollLeft | accesskit::Action::ScrollRight => portal_size.width,
            _ => portal_size.height,
        },
    };

    let delta = match event.action {
        accesskit::Action::ScrollUp => Vec2::new(0.0, -amount),
        accesskit::Action::ScrollDown => Vec2::new(0.0, amount),
        accesskit::Action::ScrollLeft => Vec2::new(-amount, 0.0),
        accesskit::Action::ScrollRight => Vec2::new(amount, 0.0),
        _ => return None,
    };

    Some(ScrollChange::Delta(delta))
}

fn viewport_pos_for_axis_progress(
    current: Point,
    portal_size: Size,
    content_size: Size,
    axis: Axis,
    progress: f64,
) -> Point {
    let scroll_range = (content_size - portal_size).max(Size::ZERO).get_coord(axis);
    let axis_pos = progress.clamp(0.0, 1.0) * scroll_range;
    match axis {
        Axis::Horizontal => Point::new(axis_pos, current.y),
        Axis::Vertical => Point::new(current.x, axis_pos),
    }
}

impl ScrollbarGeometry {
    fn new(axis: Axis, portal_size: Size, content_size: Size) -> Option<Self> {
        let portal_axis_size = portal_size.get_coord(axis);
        let content_axis_size = content_size.get_coord(axis);
        let scroll_range = content_axis_size - portal_axis_size;
        if scroll_range <= VIEWPORT_EPSILON {
            return None;
        }

        let track_rect = scrollbar_track_rect(axis, portal_size);
        let track_axis_size = track_rect.size().get_coord(axis);
        let size_ratio = (portal_axis_size / content_axis_size).clamp(0.0, 1.0);
        let cursor_length = (size_ratio * track_axis_size).max(theme::SCROLLBAR_MIN_SIZE);
        let empty_space_length = (track_axis_size - cursor_length).max(0.0);

        Some(Self {
            axis,
            track_rect,
            cursor_length,
            empty_space_length,
            scroll_range,
        })
    }

    fn thumb_rect(&self, viewport_pos: Point) -> Rect {
        let progress = scroll_progress(viewport_pos.get_coord(self.axis), self.scroll_range);
        let cursor_pos = self
            .axis
            .pack_point(progress * self.empty_space_length, 0.0);
        let cursor_size_minor = self.track_rect.size().get_coord(self.axis.cross());
        let cursor_size = self.axis.pack_size(self.cursor_length, cursor_size_minor);
        let cursor_origin = Point::new(
            self.track_rect.x0 + cursor_pos.x,
            self.track_rect.y0 + cursor_pos.y,
        );
        let (inset_x, inset_y) = self.axis.pack_xy(0.0, theme::SCROLLBAR_PAD);

        Rect::from_origin_size(cursor_origin, cursor_size).inset((-inset_x, -inset_y))
    }

    fn progress_from_mouse(&self, mouse_pos: Point, grab_anchor: f64) -> f64 {
        if self.empty_space_length <= VIEWPORT_EPSILON {
            return 0.0;
        }

        let mouse_major = mouse_pos.get_coord(self.axis);
        let (track_major, _) = self.track_rect.get_coords(self.axis);
        let new_cursor_pos_major = mouse_major - track_major - grab_anchor * self.cursor_length;

        (new_cursor_pos_major / self.empty_space_length).clamp(0.0, 1.0)
    }
}

fn scrollbar_at_pos(
    portal_size: Size,
    content_size: Size,
    pos: Point,
) -> Option<ScrollbarGeometry> {
    [Axis::Vertical, Axis::Horizontal]
        .into_iter()
        .find_map(|axis| {
            let scrollbar = ScrollbarGeometry::new(axis, portal_size, content_size)?;
            scrollbar.track_rect.contains(pos).then_some(scrollbar)
        })
}

fn scrollbar_track_rect(axis: Axis, portal_size: Size) -> Rect {
    let thickness = theme::SCROLLBAR_WIDTH + theme::SCROLLBAR_PAD * 2.0;
    let track_len = portal_size.get_coord(axis);
    let track_offset = portal_size.get_coord(axis.cross()) - thickness;
    Rect::from_origin_size(
        axis.pack_point(0.0, track_offset),
        axis.pack_size(track_len, thickness),
    )
}

fn paint_scrollbar_thumb(scene: &mut Scene, cursor_rect: Rect) {
    let cursor_rect = cursor_rect.to_rounded_rect(theme::SCROLLBAR_RADIUS);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        theme::SCROLLBAR_COLOR,
        None,
        &cursor_rect,
    );
    scene.stroke(
        &Stroke::new(theme::SCROLLBAR_EDGE_WIDTH),
        Affine::IDENTITY,
        theme::SCROLLBAR_BORDER_COLOR,
        None,
        &cursor_rect,
    );
}

fn scroll_progress(viewport_pos: f64, scroll_range: f64) -> f64 {
    if scroll_range > VIEWPORT_EPSILON {
        (viewport_pos / scroll_range).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn centered_child_origin(portal_size: Size, content_size: Size) -> Point {
    Point::new(
        ((portal_size.width - content_size.width) / 2.0).max(0.0),
        0.0,
    )
}

fn content_pos_under_cursor(viewport_pos: Point, cursor_pos: Point, child_origin: Point) -> Point {
    viewport_pos + (cursor_pos - child_origin)
}

fn sanitize_scale(scale: f64) -> f64 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

fn is_zoom_modifier(modifiers: Modifiers) -> bool {
    modifiers.ctrl() || modifiers.meta()
}

fn scroll_delta_logical_px(
    delta: ScrollDelta,
    line_logical_px: f64,
    page_size: Size,
    scale_factor: f64,
) -> Vec2 {
    let scale_factor = sanitize_scale(scale_factor);
    let line_px = PhysicalPosition {
        x: line_logical_px * scale_factor,
        y: line_logical_px * scale_factor,
    };
    let page_px = PhysicalPosition {
        x: page_size.width.max(0.0) * scale_factor,
        y: page_size.height.max(0.0) * scale_factor,
    };
    let delta_px = delta.to_pixel_delta(line_px, page_px);
    let delta = delta_px.to_logical::<f64>(scale_factor);

    Vec2::new(delta.x, delta.y)
}

fn zoom_action_from_wheel_delta(wheel_delta: &mut f64, delta_y: f64) -> Option<ZoomAction> {
    *wheel_delta += delta_y;
    if wheel_delta.abs() < ZOOM_WHEEL_THRESHOLD {
        return None;
    }

    let action = if *wheel_delta > 0.0 {
        ZoomAction::In
    } else {
        ZoomAction::Out
    };
    *wheel_delta = 0.0;
    Some(action)
}

fn anchored_viewport_after_zoom(
    anchor: ZoomAnchor,
    new_zoom_scale: f64,
    child_origin: Point,
) -> Point {
    let old_zoom_scale = sanitize_scale(anchor.old_zoom_scale);
    let new_zoom_scale = sanitize_scale(new_zoom_scale);
    let factor = new_zoom_scale / old_zoom_scale;

    Point::new(
        anchor.content_pos.x * factor - anchor.cursor_pos.x + child_origin.x,
        anchor.content_pos.y * factor - anchor.cursor_pos.y + child_origin.y,
    )
}

#[cfg(test)]
mod tests {
    use masonry::core::ScrollDelta;
    use masonry::dpi::PhysicalPosition;
    use masonry::kurbo::{Axis, Point, Size};

    use super::{
        ScrollbarGeometry, ZOOM_WHEEL_LINE_PX, ZoomAnchor, anchored_viewport_after_zoom,
        centered_child_origin, content_pos_under_cursor, scroll_delta_logical_px, scroll_progress,
        scrollbar_at_pos, zoom_action_from_wheel_delta,
    };
    use crate::doc::ZoomAction;

    #[test]
    fn zoom_scroll_delta_converts_lines_to_preview_pixels() {
        let delta = scroll_delta_logical_px(
            ScrollDelta::LineDelta(0.0, 1.0),
            ZOOM_WHEEL_LINE_PX,
            Size::new(800.0, 600.0),
            2.0,
        );

        assert_eq!(delta.y, 20.0);
    }

    #[test]
    fn zoom_scroll_delta_preserves_pixel_deltas() {
        let delta = scroll_delta_logical_px(
            ScrollDelta::PixelDelta(PhysicalPosition { x: 0.0, y: -30.0 }),
            ZOOM_WHEEL_LINE_PX,
            Size::new(800.0, 600.0),
            1.0,
        );

        assert_eq!(delta.y, -30.0);
    }

    #[test]
    fn wheel_delta_accumulates_before_zooming() {
        let mut wheel_delta = 0.0;

        assert_eq!(zoom_action_from_wheel_delta(&mut wheel_delta, 19.0), None);
        assert_eq!(
            zoom_action_from_wheel_delta(&mut wheel_delta, 1.0),
            Some(ZoomAction::In)
        );
        assert_eq!(wheel_delta, 0.0);
        assert_eq!(
            zoom_action_from_wheel_delta(&mut wheel_delta, -20.0),
            Some(ZoomAction::Out)
        );
        assert_eq!(wheel_delta, 0.0);
    }

    #[test]
    fn anchored_viewport_keeps_cursor_content_position_stable() {
        let child_origin = Point::new(20.0, 0.0);
        let anchor = ZoomAnchor {
            content_pos: Point::new(150.0, 225.0),
            cursor_pos: Point::new(50.0, 25.0),
            old_zoom_scale: 1.0,
        };

        let viewport = anchored_viewport_after_zoom(anchor, 1.3, child_origin);

        assert_close(viewport.x, 165.0);
        assert_close(viewport.y, 267.5);

        assert_close(
            anchor.content_pos.x * 1.3 - viewport.x,
            anchor.cursor_pos.x - child_origin.x,
        );
        assert_close(
            anchor.content_pos.y * 1.3 - viewport.y,
            anchor.cursor_pos.y - child_origin.y,
        );
    }

    #[test]
    fn scroll_progress_tracks_viewport_position() {
        assert_close(scroll_progress(50.0, 200.0), 0.25);
        assert_close(scroll_progress(250.0, 200.0), 1.0);
        assert_close(scroll_progress(50.0, 0.0), 0.0);
    }

    #[test]
    fn child_origin_centers_narrow_content_horizontally() {
        assert_eq!(
            centered_child_origin(Size::new(800.0, 600.0), Size::new(400.0, 1200.0)),
            Point::new(200.0, 0.0)
        );
        assert_eq!(
            centered_child_origin(Size::new(800.0, 600.0), Size::new(900.0, 1200.0)),
            Point::ORIGIN
        );
    }

    #[test]
    fn content_pos_under_cursor_accounts_for_centered_child_origin() {
        assert_eq!(
            content_pos_under_cursor(
                Point::new(0.0, 100.0),
                Point::new(300.0, 240.0),
                Point::new(200.0, 0.0),
            ),
            Point::new(100.0, 340.0)
        );
    }

    #[test]
    fn overlay_scrollbar_thumb_tracks_viewport_position() {
        let thumb = ScrollbarGeometry::new(
            Axis::Vertical,
            Size::new(100.0, 100.0),
            Size::new(100.0, 300.0),
        )
        .expect("vertical overflow should show a scrollbar")
        .thumb_rect(Point::new(0.0, 100.0));

        assert_close(thumb.y0, 27.5);
        assert_close(thumb.height(), 45.0);
    }

    #[test]
    fn overlay_scrollbar_mouse_position_maps_to_progress() {
        let scrollbar = ScrollbarGeometry::new(
            Axis::Vertical,
            Size::new(100.0, 100.0),
            Size::new(100.0, 300.0),
        )
        .expect("vertical overflow should accept scrollbar input");
        let progress = scrollbar.progress_from_mouse(Point::new(96.0, 50.0), 0.5);

        assert_close(progress, 0.5);
    }

    #[test]
    fn overlay_scrollbar_hit_testing_prefers_visible_axes() {
        let portal = Size::new(100.0, 100.0);
        let content = Size::new(300.0, 300.0);

        assert_eq!(
            scrollbar_at_pos(portal, content, Point::new(96.0, 50.0))
                .map(|scrollbar| scrollbar.axis),
            Some(Axis::Vertical)
        );
        assert_eq!(
            scrollbar_at_pos(portal, content, Point::new(50.0, 96.0))
                .map(|scrollbar| scrollbar.axis),
            Some(Axis::Horizontal)
        );
        assert_eq!(
            scrollbar_at_pos(portal, Size::new(80.0, 80.0), Point::new(96.0, 50.0))
                .map(|scrollbar| scrollbar.axis),
            None
        );
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }
}
