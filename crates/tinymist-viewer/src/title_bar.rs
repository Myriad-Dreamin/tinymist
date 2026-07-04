//! Viewer-owned title bar for decorationless native windows.

use std::any::type_name;
use std::marker::PhantomData;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, CursorIcon, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef, QueryCtx,
    RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut,
};
use masonry::dpi::{LogicalPosition, PhysicalPosition};
use masonry::layout::LenReq;
use reflexo::vector::incr::IncrDocClient;
use reflexo::vector::stream::BytesModuleStream;
use reflexo_vec2svg::IncrSvgDocServer;
use tinymist_std::typst::{TypstDocument, TypstPagedDocument};
use tinymist_std::typst_shim::syntax::VirtualPathExt;
use tracing::{Span, trace_span};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes as TypstBytes, Datetime, Duration as TypstDuration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use vello::Scene;
use vello::kurbo::{Affine, Axis, BezPath, Point, Rect, Shape, Size};
use vello::peniko::{Color, Fill};
use xilem::core::{Arg, MessageCtx, MessageResult, Mut, View, ViewArgument, ViewMarker};
use xilem::{Pod, ViewCtx};

use tinymist_viewer::incr::IncrVelloDocClient;
use tinymist_viewer::protocol::preview_update_from_bytes;

/// Height of the viewer-owned title bar, in logical pixels.
pub const TITLE_BAR_HEIGHT: f64 = 44.0;

const TITLE_TEXT_SIZE_PT: f64 = 14.0;
const TITLE_PADDING_X: f64 = 16.0;
const BUTTON_WIDTH: f64 = 46.0;
const BUTTON_GROUP_WIDTH: f64 = BUTTON_WIDTH * 4.0;
const AVG_TITLE_CHAR_WIDTH: f64 = 8.4;
const TITLE_BAR_BG: Color = Color::from_rgb8(0x29, 0x29, 0x29);
const BUTTON_ICON_COLOR: Color = Color::from_rgba8(0xf6, 0xf6, 0xf6, 0xdf);
const BUTTON_HOVER_ALPHA: f32 = 0.11;
const BUTTON_ACTIVE_ALPHA: f32 = 0.17;
const CLOSE_HOVER_ALPHA: f32 = 0.93;
const BUTTON_TRANSITION_MS: f32 = 130.0;
const BUTTON_ALPHA_EPSILON: f32 = 0.001;
const TITLE_BAR_BUTTON_COUNT: usize = 4;
const TITLE_BAR_DOUBLE_CLICK_MAX_DELAY: Duration = Duration::from_millis(500);
const TITLE_BAR_DOUBLE_CLICK_MAX_DISTANCE: f64 = 5.0;

const FA_ICON_SIZE: f64 = 13.5;
const FA_XMARK_PATH: &str = "M342.6 150.6c12.5-12.5 12.5-32.8 0-45.3s-32.8-12.5-45.3 0L192 210.7 86.6 105.4c-12.5-12.5-32.8-12.5-45.3 0s-12.5 32.8 0 45.3L146.7 256 41.4 361.4c-12.5 12.5-12.5 32.8 0 45.3s32.8 12.5 45.3 0L192 301.3 297.4 406.6c12.5 12.5 32.8 12.5 45.3 0s12.5-32.8 0-45.3L237.3 256 342.6 150.6z";
const FA_MINUS_PATH: &str = "M432 256c0 17.7-14.3 32-32 32L48 288c-17.7 0-32-14.3-32-32s14.3-32 32-32l352 0c17.7 0 32 14.3 32 32z";
const FA_WINDOW_MAXIMIZE_PATH: &str = "M32 32C14.3 32 0 46.3 0 64l0 384c0 17.7 14.3 32 32 32l448 0c17.7 0 32-14.3 32-32l0-384c0-17.7-14.3-32-32-32L32 32zM96 96l320 0 0 320L96 416 96 96z";
const FA_CIRCLE_QUESTION_PATH: &str = "M256 512A256 256 0 1 0 256 0a256 256 0 1 0 0 512zM169.8 165.3c7.9-22.3 29.1-37.3 52.8-37.3l58.3 0c34.9 0 63.1 28.3 63.1 63.1c0 22.6-12.1 43.5-31.7 54.8L280 264.4c-.2 13-10.9 23.6-24 23.6c-13.3 0-24-10.7-24-24l0-13.5c0-8.6 4.6-16.5 12.1-20.8l44.3-25.4c4.7-2.7 7.6-7.7 7.6-13.1c0-8.4-6.8-15.1-15.1-15.1l-58.3 0c-3.4 0-6.4 2.1-7.5 5.3l-.4 1.2c-4.4 12.5-18.2 19-30.6 14.6s-19-18.2-14.6-30.6l.4-1.2zM224 352a32 32 0 1 1 64 0 32 32 0 1 1 -64 0z";

/// Creates a viewer title bar with document title and window controls.
pub fn title_bar<State, Action, F>(
    title: impl Into<String>,
    on_action: F,
) -> TitleBar<State, Action, F>
where
    State: ViewArgument,
    F: Fn(Arg<'_, State>, TitleBarAction) -> Action + 'static,
{
    TitleBar {
        title: title.into(),
        on_action,
        phantom: PhantomData,
    }
}

/// An app-level action emitted by the viewer title bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleBarAction {
    /// Toggle the in-app help overlay.
    ToggleHelp,
}

/// The Xilem view for [`TitleBarWidget`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct TitleBar<State, Action, F> {
    title: String,
    on_action: F,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<State, Action, F> ViewMarker for TitleBar<State, Action, F> {}

impl<State, Action, F> View<State, Action, ViewCtx> for TitleBar<State, Action, F>
where
    State: ViewArgument,
    Action: 'static,
    F: Fn(Arg<'_, State>, TitleBarAction) -> Action + 'static,
{
    type Element = Pod<TitleBarWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: Arg<'_, State>) -> (Self::Element, Self::ViewState) {
        (
            ctx.with_action_widget(|ctx| ctx.create_pod(TitleBarWidget::new(self.title.clone()))),
            (),
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: Arg<'_, State>,
    ) {
        if self.title != prev.title {
            TitleBarWidget::set_title(&mut element, self.title.clone());
        }
    }

    fn teardown(
        &self,
        (): &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        app_state: Arg<'_, State>,
    ) -> MessageResult<Action> {
        match message.take_message::<TitleBarAction>() {
            Some(action) => MessageResult::Action((self.on_action)(app_state, *action)),
            None => {
                tracing::error!(
                    "Wrong message type in TitleBar::message: {message:?} expected {}",
                    type_name::<TitleBarAction>()
                );
                MessageResult::Stale
            }
        }
    }
}

/// A decorationless-window title bar.
pub struct TitleBarWidget {
    title: String,
    size: Size,
    title_scene: Option<TitleScene>,
    hovered_button: Option<TitleBarButton>,
    active_button: Option<TitleBarButton>,
    button_visuals: [ButtonVisual; TITLE_BAR_BUTTON_COUNT],
    last_title_bar_click: Option<TitleBarClick>,
}

#[derive(Clone)]
struct TitleScene {
    title: String,
    width: f64,
    height: f64,
    scene: Arc<Scene>,
}

#[derive(Clone, Copy)]
struct TitleBarClick {
    time: Instant,
    pos: Point,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TitleBarButton {
    Help,
    Close,
    Maximize,
    Minimize,
}

impl TitleBarButton {
    fn paint_order() -> [Self; 4] {
        [Self::Help, Self::Minimize, Self::Maximize, Self::Close]
    }

    fn visual_index(self) -> usize {
        match self {
            Self::Help => 0,
            Self::Minimize => 1,
            Self::Maximize => 2,
            Self::Close => 3,
        }
    }

    fn right_index(self) -> f64 {
        match self {
            Self::Close => 0.0,
            Self::Maximize => 1.0,
            Self::Minimize => 2.0,
            Self::Help => 3.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ButtonVisual {
    alpha: f32,
    target_alpha: f32,
    alpha_per_ms: f32,
}

impl Default for ButtonVisual {
    fn default() -> Self {
        Self {
            alpha: 0.0,
            target_alpha: 0.0,
            alpha_per_ms: 0.0,
        }
    }
}

impl ButtonVisual {
    fn move_to(&mut self, target_alpha: f32) -> bool {
        let target_alpha = target_alpha.clamp(0.0, 1.0);
        if (self.target_alpha - target_alpha).abs() <= BUTTON_ALPHA_EPSILON {
            if !self.is_stable() && self.alpha_per_ms.abs() <= BUTTON_ALPHA_EPSILON {
                self.alpha_per_ms = (self.target_alpha - self.alpha) / BUTTON_TRANSITION_MS;
            }

            if !self.is_stable() {
                return true;
            }

            return false;
        }

        self.target_alpha = target_alpha;
        self.alpha_per_ms = (self.target_alpha - self.alpha) / BUTTON_TRANSITION_MS;
        true
    }

    fn advance(&mut self, millis: f32) -> bool {
        if self.is_stable() {
            return false;
        }

        let previous = self.alpha;
        self.alpha += self.alpha_per_ms * millis.max(0.0);
        let passed_target = (previous <= self.target_alpha && self.alpha >= self.target_alpha)
            || (previous >= self.target_alpha && self.alpha <= self.target_alpha);
        if passed_target || (self.alpha - self.target_alpha).abs() <= BUTTON_ALPHA_EPSILON {
            self.alpha = self.target_alpha;
            self.alpha_per_ms = 0.0;
        }

        (self.alpha - previous).abs() > BUTTON_ALPHA_EPSILON
    }

    fn is_stable(self) -> bool {
        (self.alpha - self.target_alpha).abs() <= BUTTON_ALPHA_EPSILON
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FontAwesomeIcon {
    CircleQuestion,
    Xmark,
    WindowMaximize,
    Minus,
}

impl FontAwesomeIcon {
    fn path(self) -> &'static BezPath {
        match self {
            Self::CircleQuestion => {
                static PATH: OnceLock<BezPath> = OnceLock::new();
                PATH.get_or_init(|| parse_font_awesome_path(FA_CIRCLE_QUESTION_PATH))
            }
            Self::Xmark => {
                static PATH: OnceLock<BezPath> = OnceLock::new();
                PATH.get_or_init(|| parse_font_awesome_path(FA_XMARK_PATH))
            }
            Self::WindowMaximize => {
                static PATH: OnceLock<BezPath> = OnceLock::new();
                PATH.get_or_init(|| parse_font_awesome_path(FA_WINDOW_MAXIMIZE_PATH))
            }
            Self::Minus => {
                static PATH: OnceLock<BezPath> = OnceLock::new();
                PATH.get_or_init(|| parse_font_awesome_path(FA_MINUS_PATH))
            }
        }
    }

    fn target_size(self) -> f64 {
        match self {
            Self::CircleQuestion => 14.5,
            Self::WindowMaximize => 13.5,
            Self::Minus | Self::Xmark => FA_ICON_SIZE,
        }
    }
}

fn parse_font_awesome_path(path: &'static str) -> BezPath {
    BezPath::from_svg(path).expect("embedded Font Awesome SVG path must parse")
}

impl TitleBarWidget {
    /// Creates a title bar widget.
    pub fn new(title: String) -> Self {
        Self {
            title,
            size: Size::ZERO,
            title_scene: None,
            hovered_button: None,
            active_button: None,
            button_visuals: [ButtonVisual::default(); TITLE_BAR_BUTTON_COUNT],
            last_title_bar_click: None,
        }
    }

    /// Updates the visible title text.
    pub fn set_title(this: &mut WidgetMut<'_, Self>, title: String) {
        if this.widget.title == title {
            return;
        }

        this.widget.title = title;
        this.widget.title_scene = None;
        this.ctx.request_render();
        this.ctx.request_accessibility_update();
    }

    fn update_hovered_button(&mut self, ctx: &mut EventCtx<'_>, pos: Point) {
        let hovered = button_at_pos(self.size, pos);
        if self.hovered_button != hovered {
            self.hovered_button = hovered;
            self.update_button_visual_targets_event_ctx(ctx);
        }
    }

    fn activate_button(&mut self, ctx: &mut EventCtx<'_>, button: TitleBarButton) {
        match button {
            TitleBarButton::Help => {
                ctx.submit_action::<TitleBarAction>(TitleBarAction::ToggleHelp);
            }
            TitleBarButton::Close => ctx.exit(),
            TitleBarButton::Maximize => {
                if !crate::native_title_bar::take_native_maximize_activation() {
                    ctx.toggle_maximized();
                }
            }
            TitleBarButton::Minimize => ctx.minimize(),
        }
    }

    fn update_button_visual_targets_event_ctx(&mut self, ctx: &mut EventCtx<'_>) {
        if self.update_button_visual_targets() {
            ctx.request_paint_only();
            ctx.request_anim_frame();
        }
    }

    fn update_button_visual_targets_update_ctx(&mut self, ctx: &mut UpdateCtx<'_>) {
        if self.update_button_visual_targets() {
            ctx.request_paint_only();
            ctx.request_anim_frame();
        }
    }

    fn update_button_visual_targets(&mut self) -> bool {
        let mut changed = false;
        for button in TitleBarButton::paint_order() {
            let target = button_background_alpha(
                button,
                self.hovered_button == Some(button),
                self.active_button == Some(button),
            );
            changed |= self.button_visuals[button.visual_index()].move_to(target);
        }
        changed
    }

    fn clear_active_button_event_ctx(&mut self, ctx: &mut EventCtx<'_>) -> bool {
        if self.active_button.take().is_some() {
            ctx.release_pointer();
            self.update_button_visual_targets_event_ctx(ctx);
            true
        } else {
            false
        }
    }

    fn handle_title_bar_primary_down(&mut self, ctx: &mut EventCtx<'_>, pos: Point) {
        let now = Instant::now();
        if is_title_bar_double_click(self.last_title_bar_click, now, pos) {
            self.last_title_bar_click = None;
            ctx.toggle_maximized();
        } else {
            self.last_title_bar_click = Some(TitleBarClick { time: now, pos });
            ctx.drag_window();
        }
    }

    fn title_scene(&mut self) -> Option<&Scene> {
        let width = title_available_width(self.size);
        let height = self.size.height;
        if width <= 1.0 || height <= 1.0 {
            return None;
        }

        let title = truncate_title_for_width(&self.title, width);
        let reuse = self.title_scene.as_ref().is_some_and(|scene| {
            scene.title == title && scene.width == width && scene.height == height
        });
        if !reuse {
            match render_title_scene(&title, width, height) {
                Ok(scene) => {
                    self.title_scene = Some(TitleScene {
                        title,
                        width,
                        height,
                        scene,
                    });
                }
                Err(err) => {
                    log::warn!("failed to render title bar text with Typst: {err}");
                    self.title_scene = None;
                    return None;
                }
            }
        }

        self.title_scene.as_ref().map(|scene| &*scene.scene)
    }
}

impl Widget for TitleBarWidget {
    type Action = TitleBarAction;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Secondary),
                state,
                ..
            }) => {
                let pos = ctx.local_position(state.position);
                if button_at_pos(self.size, pos).is_none() {
                    self.last_title_bar_click = None;
                    ctx.show_window_menu(window_menu_position(
                        state.position,
                        ctx.get_scale_factor(),
                    ));
                    ctx.set_handled();
                }
            }
            PointerEvent::Down(PointerButtonEvent {
                button: None | Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let pos = ctx.local_position(state.position);
                if let Some(button) = button_at_pos(self.size, pos) {
                    self.last_title_bar_click = None;
                    self.active_button = Some(button);
                    self.hovered_button = Some(button);
                    ctx.capture_pointer();
                    self.update_button_visual_targets_event_ctx(ctx);
                } else {
                    self.handle_title_bar_primary_down(ctx, pos);
                }
                ctx.set_handled();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let pos = ctx.local_position(current.position);
                self.update_hovered_button(ctx, pos);
            }
            PointerEvent::Up(event) => {
                let pos = ctx.local_position(event.state.position);
                let hovered = button_at_pos(self.size, pos);
                if let Some(active) = self.active_button.take() {
                    ctx.release_pointer();
                    self.update_button_visual_targets_event_ctx(ctx);
                    if hovered == Some(active) {
                        self.activate_button(ctx, active);
                    }
                    ctx.set_handled();
                }
                if self.hovered_button != hovered {
                    self.hovered_button = hovered;
                    self.update_button_visual_targets_event_ctx(ctx);
                }
            }
            PointerEvent::Cancel(_) => {
                if self.clear_active_button_event_ctx(ctx) {
                    ctx.set_handled();
                }
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &TextEvent,
    ) {
    }

    fn accepts_pointer_interaction(&self) -> bool {
        true
    }

    fn get_cursor(&self, ctx: &QueryCtx<'_>, pos: Point) -> CursorIcon {
        if button_at_pos(self.size, ctx.to_local(pos)).is_some() {
            CursorIcon::Pointer
        } else {
            CursorIcon::Default
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        let millis = (interval as f64 / 1_000_000.0) as f32;
        let mut changed = false;
        let mut ongoing = false;
        for visual in &mut self.button_visuals {
            changed |= visual.advance(millis);
            ongoing |= !visual.is_stable();
        }

        if changed {
            ctx.request_paint_only();
        }
        if ongoing {
            ctx.request_anim_frame();
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::HoveredChanged(false) => {
                if self.hovered_button.take().is_some() {
                    self.update_button_visual_targets_update_ctx(ctx);
                }
            }
            Update::ActiveChanged(false) => {
                if self.active_button.take().is_some() {
                    self.update_button_visual_targets_update_ctx(ctx);
                }
            }
            _ => {}
        }
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        _cross_length: Option<f64>,
    ) -> f64 {
        match axis {
            Axis::Horizontal => match len_req {
                LenReq::FitContent(space) => space,
                LenReq::MinContent => 0.0,
                LenReq::MaxContent => BUTTON_GROUP_WIDTH + TITLE_PADDING_X * 2.0,
            },
            Axis::Vertical => TITLE_BAR_HEIGHT,
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        if self.size != size {
            self.size = size;
            self.title_scene = None;
        }
        ctx.set_clip_path(size.to_rect());
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, scene: &mut Scene) {
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            TITLE_BAR_BG,
            None,
            &self.size.to_rect(),
        );
        if let Some(title_scene) = self.title_scene() {
            scene.append(title_scene, Some(Affine::translate((TITLE_PADDING_X, 0.0))));
        }
        for button in TitleBarButton::paint_order() {
            paint_title_bar_button(
                scene,
                self.size,
                button,
                self.button_visuals[button.visual_index()].alpha,
                self.hovered_button == Some(button) || self.active_button == Some(button),
            );
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::TitleBar
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.title.clone());
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn make_trace_span(&self, widget_id: WidgetId) -> Span {
        trace_span!("TitleBar", id = widget_id.trace())
    }
}

fn title_available_width(size: Size) -> f64 {
    (size.width - BUTTON_GROUP_WIDTH - TITLE_PADDING_X * 2.0).max(0.0)
}

pub(crate) fn button_at_pos(size: Size, pos: Point) -> Option<TitleBarButton> {
    TitleBarButton::paint_order()
        .into_iter()
        .find(|button| button_rect(size, *button).contains(pos))
}

fn button_rect(size: Size, button: TitleBarButton) -> Rect {
    let x1 = size.width - button.right_index() * BUTTON_WIDTH;
    Rect::new(x1 - BUTTON_WIDTH, 0.0, x1, size.height)
}

fn window_menu_position(pos: PhysicalPosition<f64>, scale_factor: f64) -> LogicalPosition<f64> {
    let scale_factor = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    LogicalPosition::new(pos.x / scale_factor, pos.y / scale_factor)
}

fn is_title_bar_double_click(last: Option<TitleBarClick>, now: Instant, pos: Point) -> bool {
    let Some(last) = last else {
        return false;
    };

    now.duration_since(last.time) <= TITLE_BAR_DOUBLE_CLICK_MAX_DELAY
        && (pos - last.pos).hypot2()
            <= TITLE_BAR_DOUBLE_CLICK_MAX_DISTANCE * TITLE_BAR_DOUBLE_CLICK_MAX_DISTANCE
}

fn paint_title_bar_button(
    scene: &mut Scene,
    size: Size,
    button: TitleBarButton,
    background_alpha: f32,
    hot: bool,
) {
    let rect = button_rect(size, button);
    if background_alpha > BUTTON_ALPHA_EPSILON {
        let color = button_background_color(button, background_alpha);
        scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &rect);
    }

    let icon_color = if hot && button == TitleBarButton::Close {
        Color::WHITE
    } else {
        BUTTON_ICON_COLOR
    };
    match button {
        TitleBarButton::Help => paint_help_icon(scene, rect, icon_color),
        TitleBarButton::Close => paint_close_icon(scene, rect, icon_color),
        TitleBarButton::Maximize => paint_maximize_icon(scene, rect, icon_color),
        TitleBarButton::Minimize => paint_minimize_icon(scene, rect, icon_color),
    }
}

fn button_background_alpha(button: TitleBarButton, hovered: bool, active: bool) -> f32 {
    if !hovered && !active {
        return 0.0;
    }

    if button == TitleBarButton::Close {
        CLOSE_HOVER_ALPHA
    } else if active {
        BUTTON_ACTIVE_ALPHA
    } else {
        BUTTON_HOVER_ALPHA
    }
}

fn button_background_color(button: TitleBarButton, alpha: f32) -> Color {
    let alpha = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    if button == TitleBarButton::Close {
        Color::from_rgba8(0xe8, 0x11, 0x23, alpha)
    } else {
        Color::from_rgba8(0xff, 0xff, 0xff, alpha)
    }
}

fn paint_font_awesome_icon(scene: &mut Scene, rect: Rect, color: Color, icon: FontAwesomeIcon) {
    let path = icon.path();
    let bounds = path.bounding_box();
    let scale = icon.target_size() / bounds.width().max(bounds.height()).max(1.0);
    let x = (rect.x0 + rect.x1 - bounds.width() * scale) * 0.5 - bounds.x0 * scale;
    let y = (rect.y0 + rect.y1 - bounds.height() * scale) * 0.5 - bounds.y0 * scale;
    scene.fill(
        Fill::NonZero,
        Affine::translate((x, y)) * Affine::scale(scale),
        color,
        None,
        path,
    );
}

fn paint_minimize_icon(scene: &mut Scene, rect: Rect, color: Color) {
    paint_font_awesome_icon(scene, rect, color, FontAwesomeIcon::Minus);
}

fn paint_maximize_icon(scene: &mut Scene, rect: Rect, color: Color) {
    paint_font_awesome_icon(scene, rect, color, FontAwesomeIcon::WindowMaximize);
}

fn paint_close_icon(scene: &mut Scene, rect: Rect, color: Color) {
    paint_font_awesome_icon(scene, rect, color, FontAwesomeIcon::Xmark);
}

fn paint_help_icon(scene: &mut Scene, rect: Rect, color: Color) {
    paint_font_awesome_icon(scene, rect, color, FontAwesomeIcon::CircleQuestion);
}

fn truncate_title_for_width(title: &str, width: f64) -> String {
    if width <= AVG_TITLE_CHAR_WIDTH * 4.0 {
        return String::new();
    }

    let max_chars = (width / AVG_TITLE_CHAR_WIDTH).floor() as usize;
    let mut chars = title.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() && max_chars > 3 {
        let keep = max_chars.saturating_sub(3);
        format!("{}...", truncated.chars().take(keep).collect::<String>())
    } else {
        title.to_owned()
    }
}

fn render_title_scene(title: &str, width: f64, height: f64) -> Result<Arc<Scene>> {
    let source = Source::new(
        FileId::new(RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("/tinymist-viewer-title.typ")
                .expect("title source path should be valid"),
        )),
        title_typst_source(title, width, height),
    );
    let world = TitleWorld { main: source };
    let compiled = typst::compile::<TypstPagedDocument>(&world);
    for warning in compiled.warnings {
        log::debug!("Typst title render warning: {warning:?}");
    }
    let doc = compiled
        .output
        .map_err(|errors| anyhow!("failed to compile title text: {errors:?}"))?;
    let document = TypstDocument::Paged(Arc::new(doc));
    let mut renderer = IncrSvgDocServer::default();
    let frame = renderer.pack_delta(&document);
    let update = preview_update_from_bytes(&frame).context("title preview frame is invalid")?;

    let mut doc = IncrDocClient::default();
    let mut vello = IncrVelloDocClient::default();
    if update.reset_before_merge {
        doc = IncrDocClient::default();
        vello.reset();
    }
    let delta = BytesModuleStream::from_slice(update.payload).checkout_owned();
    doc.merge_delta(delta);

    let mut pages = vello.render_pages(&mut doc)?;
    let (scene, _) = pages
        .pop()
        .context("Typst title render produced no pages")?;
    Ok(scene)
}

fn title_typst_source(title: &str, width: f64, height: f64) -> String {
    let title = typst_string_literal(title);
    format!(
        r##"#set page(width: {width}pt, height: {height}pt, margin: 0pt)
#set text(font: "Libertinus Serif", size: {TITLE_TEXT_SIZE_PT}pt, fill: rgb("#f2f2f2"))
#place(left + horizon, text({title}))
"##
    )
}

fn typst_string_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' | '\r' | '\t' => out.push(' '),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

struct TitleWorld {
    main: Source,
}

impl World for TitleWorld {
    fn library(&self) -> &LazyHash<Library> {
        &title_typst_base().library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &title_typst_base().book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotFound(
                id.vpath().as_rooted_path_compat().to_owned(),
            ))
        }
    }

    fn file(&self, id: FileId) -> FileResult<TypstBytes> {
        Err(FileError::NotFound(
            id.vpath().as_rooted_path_compat().to_owned(),
        ))
    }

    fn font(&self, index: usize) -> Option<Font> {
        title_typst_base().fonts.get(index).cloned()
    }

    fn today(&self, _: Option<TypstDuration>) -> Option<Datetime> {
        Some(Datetime::from_ymd(1970, 1, 1).expect("valid deterministic date"))
    }
}

struct TitleTypstBase {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

fn title_typst_base() -> &'static TitleTypstBase {
    static BASE: OnceLock<TitleTypstBase> = OnceLock::new();
    BASE.get_or_init(|| {
        let fonts = typst_assets::fonts()
            .flat_map(|data| Font::iter(TypstBytes::new(data)))
            .collect::<Vec<_>>();

        TitleTypstBase {
            library: LazyHash::new(Library::builder().build()),
            book: LazyHash::new(FontBook::from_fonts(&fonts)),
            fonts,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_controls_are_ordered_from_help_to_close() {
        let size = Size::new(BUTTON_GROUP_WIDTH, TITLE_BAR_HEIGHT);
        let y = TITLE_BAR_HEIGHT * 0.5;

        assert_eq!(
            button_at_pos(size, Point::new(22.0, y)),
            Some(TitleBarButton::Help)
        );
        assert_eq!(
            button_at_pos(size, Point::new(66.0, y)),
            Some(TitleBarButton::Minimize)
        );
        assert_eq!(
            button_at_pos(size, Point::new(110.0, y)),
            Some(TitleBarButton::Maximize)
        );
        assert_eq!(
            button_at_pos(size, Point::new(154.0, y)),
            Some(TitleBarButton::Close)
        );
    }

    #[test]
    fn embedded_font_awesome_paths_parse_to_nonempty_bounds() {
        for icon in [
            FontAwesomeIcon::CircleQuestion,
            FontAwesomeIcon::Xmark,
            FontAwesomeIcon::WindowMaximize,
            FontAwesomeIcon::Minus,
        ] {
            let bounds = icon.path().bounding_box();
            assert!(bounds.width() > 0.0, "{icon:?} should have width");
            assert!(bounds.height() > 0.0, "{icon:?} should have height");
        }
    }

    #[test]
    fn title_text_uses_libertinus_serif_at_ui_size() {
        let source = title_typst_source("main.typ", 240.0, TITLE_BAR_HEIGHT);

        assert!(source.contains(r#"font: "Libertinus Serif""#));
        assert!(source.contains("size: 14pt"));
    }

    #[test]
    fn button_background_targets_distinguish_hover_active_and_close() {
        assert_eq!(
            button_background_alpha(TitleBarButton::Minimize, false, false),
            0.0
        );
        assert_eq!(
            button_background_alpha(TitleBarButton::Minimize, true, false),
            BUTTON_HOVER_ALPHA
        );
        assert_eq!(
            button_background_alpha(TitleBarButton::Minimize, true, true),
            BUTTON_ACTIVE_ALPHA
        );
        assert_eq!(
            button_background_alpha(TitleBarButton::Close, true, false),
            CLOSE_HOVER_ALPHA
        );
    }

    #[test]
    fn button_visual_reschedules_unfinished_same_target() {
        let mut visual = ButtonVisual::default();
        assert!(visual.move_to(BUTTON_HOVER_ALPHA));

        visual.alpha = BUTTON_HOVER_ALPHA * 0.5;
        visual.alpha_per_ms = 0.0;

        assert!(visual.move_to(BUTTON_HOVER_ALPHA));
        assert!(visual.alpha_per_ms > 0.0);
    }

    #[test]
    fn title_bar_double_click_requires_nearby_recent_click() {
        let now = Instant::now();
        let first = TitleBarClick {
            time: now,
            pos: Point::new(20.0, 20.0),
        };

        assert!(is_title_bar_double_click(
            Some(first),
            now + Duration::from_millis(200),
            Point::new(23.0, 22.0),
        ));
        assert!(!is_title_bar_double_click(
            Some(first),
            now + TITLE_BAR_DOUBLE_CLICK_MAX_DELAY + Duration::from_millis(1),
            Point::new(20.0, 20.0),
        ));
        assert!(!is_title_bar_double_click(
            Some(first),
            now + Duration::from_millis(200),
            Point::new(40.0, 20.0),
        ));
    }

    #[test]
    fn window_menu_position_converts_physical_to_logical() {
        assert_eq!(
            window_menu_position(PhysicalPosition::new(300.0, 150.0), 1.5),
            LogicalPosition::new(200.0, 100.0),
        );
    }

    #[test]
    fn title_scene_renders_with_configured_font() {
        render_title_scene("main.typ - Tinymist View", 320.0, TITLE_BAR_HEIGHT)
            .expect("title text should render with the configured font");
    }
}
