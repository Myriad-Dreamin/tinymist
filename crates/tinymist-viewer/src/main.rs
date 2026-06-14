//! Renders and views typst document with Xilem & Vello.

use core::fmt;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use ezsockets::ClientConfig;
use ezsockets::client::ClientCloseMode;
use masonry::layout::Length;
use masonry::layout::UnitPoint;
use masonry::properties::Gap;
use masonry::theme::default_property_set;
use masonry_winit::app::{AppDriver, MasonryState, MasonryUserEvent};
use reflexo::debug_loc::DocumentPosition;
use reflexo::vector::incr::IncrDocClient;
use reflexo::vector::stream::BytesModuleStream;
use reflexo_vec2svg::IncrSvgDocServer;
use serde::{Deserialize, Serialize};
use tinymist_std::typst::{TypstDocument, TypstPagedDocument};
use tokio::sync::mpsc;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes as TypstBytes, Datetime, Duration as TypstDuration};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{DeviceEvent, StartCause, WindowEvent as WinitWindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId as WinitWindowId;
use xilem::core::{Edit, MessageProxy, fork};
use xilem::style::Style as _;
use xilem::vello::Scene;
use xilem::vello::kurbo::{Point, Size};
use xilem::vello::peniko::Color;
use xilem::view::{
    FlexExt as _, ZStackExt as _, flex_col, resize_observer, sized_box, task, zstack,
};
use xilem::{AppState, EventLoop, WidgetView, WindowId, Xilem, window};

mod native_title_bar;
mod title_bar;

use tinymist_preview::{
    ViewerWindowState as ControlPlaneViewerWindowState, ViewerWindowStateMessage,
};
use tinymist_viewer::doc::{ZoomAction, doc};
use tinymist_viewer::incr::{IncrVelloDocClient, RenderedPage};
use tinymist_viewer::protocol::preview_update_from_bytes;
use tinymist_viewer::zoom_portal::zoom_portal;
use title_bar::{TITLE_BAR_HEIGHT, TitleBarAction, title_bar};

const DEFAULT_ZOOM_SCALE: f64 = 1.0;
const MIN_ZOOM_SCALE: f64 = 0.1;
const MAX_ZOOM_SCALE: f64 = 10.0;
const ZOOM_FACTOR_EPSILON: f64 = 1e-9;
const ZOOM_FACTORS: [f64; 31] = [
    0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.3, 1.5, 1.7, 1.9, 2.1, 2.4, 2.7, 3.0,
    3.3, 3.7, 4.1, 4.6, 5.1, 5.7, 6.3, 7.0, 7.7, 8.5, 9.4, 10.0,
];

const RECONNECT_INTERVAL: Duration = Duration::from_secs(1);
const STATUS_BANNER_HEIGHT: f64 = 28.0;
const EMPTY_STATUS_BANNER_HEIGHT: f64 = 34.0;
const DEFAULT_VIEWER_INNER_WIDTH: u32 = 800;
const DEFAULT_VIEWER_CONTENT_HEIGHT: u32 = 800;
const MIN_VIEWER_INNER_WIDTH: u32 = 800;
const MIN_VIEWER_INNER_HEIGHT: u32 = DEFAULT_VIEWER_CONTENT_HEIGHT + TITLE_BAR_HEIGHT as u32;
const VIEWER_WINDOW_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Parser)]
struct Args {
    /// The address of the preview server.
    #[clap(
        long = "data-plane-host",
        default_value = "127.0.0.1:23625",
        value_name = "HOST",
        hide(true)
    )]
    pub data_plane_host: String,

    /// The document title shown in the viewer window.
    #[clap(long = "document-title", value_name = "TITLE")]
    pub document_title: Option<String>,

    /// Initial native window inner size, for example 960x1080.
    #[clap(
        long = "initial-window-inner-size",
        value_name = "WIDTHxHEIGHT",
        value_parser = parse_initial_window_inner_size
    )]
    pub initial_window_inner_size: Option<PhysicalSize<u32>>,

    /// Initial native window outer position, for example 960,0.
    #[clap(
        long = "initial-window-position",
        value_name = "X,Y",
        allow_hyphen_values = true,
        value_parser = parse_initial_window_position
    )]
    pub initial_window_position: Option<PhysicalPosition<i32>>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    env_logger::builder()
        .filter_module("tinymist", log::LevelFilter::Info)
        .try_init()?;

    let (tx, rx) = mpsc::unbounded_channel();
    let (window_state_tx, window_state_rx) = mpsc::unbounded_channel();

    let initial_window_state = ViewerWindowState::from_initial_geometry(
        args.initial_window_inner_size,
        args.initial_window_position,
    );
    let default_size = Size::new(
        f64::from(DEFAULT_VIEWER_INNER_WIDTH),
        f64::from(DEFAULT_VIEWER_CONTENT_HEIGHT),
    );
    let data_plane_host = args.data_plane_host;
    let document_title = normalize_document_title(args.document_title);
    let app = Xilem::new(
        PreviewState {
            data_plane_host,
            document_title,
            window_id: WindowId::next(),
            initial_window_state,
            running: true,
            pages: vec![],
            background_color: None,
            window_size: default_size,
            connection: ConnectionStatus::Connecting,
            status_scene: None,
            help_overlay_visible: false,
            help_overlay_scene: None,
            zoom_scale: DEFAULT_ZOOM_SCALE,
            tx,
            rx: Some(rx),
            window_state_rx: Some(window_state_rx),
        },
        PreviewState::windows,
    );

    let event_loop = EventLoop::with_user_event()
        .build()
        .context("Couldn't build event loop")?;
    let proxy = event_loop.create_proxy();
    let (driver, windows) =
        app.into_driver_and_windows(move |event| proxy.send_event(event).map_err(|err| err.0));
    let masonry_state =
        MasonryState::new(event_loop.create_proxy(), windows, default_property_set());
    let mut app = ViewerEventLoopApp {
        masonry_state,
        app_driver: Box::new(driver),
        window_state: initial_window_state,
        window_state_tx: Some(window_state_tx),
    };
    event_loop
        .run_app(&mut app)
        .context("Couldn't run event loop")?;
    Ok(())
}

struct ViewerEventLoopApp {
    masonry_state: MasonryState<'static>,
    app_driver: Box<dyn AppDriver>,
    window_state: ViewerWindowState,
    window_state_tx: Option<mpsc::UnboundedSender<ViewerWindowState>>,
}

impl ViewerEventLoopApp {
    fn record_window_event(&mut self, event: &WinitWindowEvent) {
        let changed = match event {
            WinitWindowEvent::Moved(position) => self.window_state.set_outer_position(*position),
            WinitWindowEvent::Resized(size) => self.window_state.set_inner_size(*size),
            _ => false,
        };

        if changed {
            self.send_window_state();
        }
    }

    fn send_window_state(&mut self) {
        if let Some(tx) = &self.window_state_tx
            && tx.send(self.window_state).is_err()
        {
            self.window_state_tx = None;
        }
    }
}

impl ApplicationHandler<MasonryUserEvent> for ViewerEventLoopApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry_state
            .handle_resumed(event_loop, self.app_driver.as_mut());
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry_state.handle_suspended(event_loop);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry_state.handle_about_to_wait(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WinitWindowId,
        event: WinitWindowEvent,
    ) {
        native_title_bar::install_for_window_id(window_id);
        self.record_window_event(&event);
        self.masonry_state.handle_window_event(
            event_loop,
            window_id,
            event,
            self.app_driver.as_mut(),
        );
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: MasonryUserEvent) {
        self.masonry_state
            .handle_user_event(event_loop, event, self.app_driver.as_mut());
    }

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        self.masonry_state.handle_device_event(
            event_loop,
            device_id,
            event,
            self.app_driver.as_mut(),
        );
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        self.masonry_state.handle_new_events(event_loop, cause);
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry_state.handle_exiting(event_loop);
    }

    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
        self.masonry_state.handle_memory_warning(event_loop);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct ViewerWindowState {
    #[serde(default = "default_viewer_inner_width")]
    inner_width: u32,
    #[serde(default = "default_viewer_inner_height")]
    inner_height: u32,
    #[serde(default)]
    outer_x: Option<i32>,
    #[serde(default)]
    outer_y: Option<i32>,
}

impl Default for ViewerWindowState {
    fn default() -> Self {
        Self {
            inner_width: default_viewer_inner_width(),
            inner_height: default_viewer_inner_height(),
            outer_x: None,
            outer_y: None,
        }
    }
}

impl ViewerWindowState {
    fn from_initial_geometry(
        size: Option<PhysicalSize<u32>>,
        position: Option<PhysicalPosition<i32>>,
    ) -> Self {
        let mut state = Self::default();
        if let Some(size) = size {
            state.inner_width = size.width.max(1);
            state.inner_height = size.height.max(1);
        }
        if let Some(position) = position {
            state.outer_x = Some(position.x);
            state.outer_y = Some(position.y);
        }
        state
    }

    fn inner_size(self) -> PhysicalSize<u32> {
        PhysicalSize::new(self.inner_width, self.inner_height)
    }

    fn outer_position(self) -> Option<PhysicalPosition<i32>> {
        Some(PhysicalPosition::new(self.outer_x?, self.outer_y?))
    }

    fn set_inner_size(&mut self, size: PhysicalSize<u32>) -> bool {
        if !is_valid_window_inner_size(size) {
            return false;
        }

        if self.inner_width == size.width && self.inner_height == size.height {
            return false;
        }

        self.inner_width = size.width;
        self.inner_height = size.height;
        true
    }

    fn set_outer_position(&mut self, position: PhysicalPosition<i32>) -> bool {
        if self.outer_x == Some(position.x) && self.outer_y == Some(position.y) {
            return false;
        }

        self.outer_x = Some(position.x);
        self.outer_y = Some(position.y);
        true
    }
}

fn default_viewer_inner_width() -> u32 {
    DEFAULT_VIEWER_INNER_WIDTH
}

fn default_viewer_inner_height() -> u32 {
    MIN_VIEWER_INNER_HEIGHT
}

fn is_valid_window_inner_size(size: PhysicalSize<u32>) -> bool {
    size.width >= MIN_VIEWER_INNER_WIDTH && size.height >= MIN_VIEWER_INNER_HEIGHT
}

fn viewer_window_state_payload(state: ViewerWindowState) -> serde_json::Result<String> {
    serde_json::to_string(&ViewerWindowStateMessage {
        schema_version: VIEWER_WINDOW_STATE_SCHEMA_VERSION,
        window: ControlPlaneViewerWindowState {
            inner_width: state.inner_width,
            inner_height: state.inner_height,
            outer_x: state.outer_x,
            outer_y: state.outer_y,
        },
    })
}

fn parse_initial_window_inner_size(value: &str) -> Result<PhysicalSize<u32>, String> {
    let (width, height) = value
        .split_once('x')
        .or_else(|| value.split_once('X'))
        .ok_or_else(|| "expected WIDTHxHEIGHT".to_owned())?;
    let width = parse_positive_u32(width, "width")?;
    let height = parse_positive_u32(height, "height")?;
    Ok(PhysicalSize::new(width, height))
}

fn parse_initial_window_position(value: &str) -> Result<PhysicalPosition<i32>, String> {
    let (x, y) = value
        .split_once(',')
        .ok_or_else(|| "expected X,Y".to_owned())?;
    let x = x
        .trim()
        .parse::<i32>()
        .map_err(|err| format!("invalid x position: {err}"))?;
    let y = y
        .trim()
        .parse::<i32>()
        .map_err(|err| format!("invalid y position: {err}"))?;
    Ok(PhysicalPosition::new(x, y))
}

fn parse_positive_u32(value: &str, name: &str) -> Result<u32, String> {
    let value = value
        .trim()
        .parse::<u32>()
        .map_err(|err| format!("invalid {name}: {err}"))?;
    if value == 0 {
        return Err(format!("{name} must be greater than zero"));
    }
    Ok(value)
}

struct PreviewState {
    data_plane_host: String,
    document_title: Option<String>,
    window_id: WindowId,
    initial_window_state: ViewerWindowState,
    running: bool,
    pages: Vec<RenderedPage>,
    background_color: Option<Color>,
    window_size: Size,
    connection: ConnectionStatus,
    status_scene: Option<StatusScene>,
    help_overlay_visible: bool,
    help_overlay_scene: Option<HelpOverlayScene>,
    zoom_scale: f64,
    tx: mpsc::UnboundedSender<PreviewEvent>,
    rx: Option<mpsc::UnboundedReceiver<PreviewEvent>>,
    window_state_rx: Option<mpsc::UnboundedReceiver<ViewerWindowState>>,
}

impl AppState for PreviewState {
    fn keep_running(&self) -> bool {
        self.running
    }
}

impl PreviewState {
    fn windows(&mut self) -> impl Iterator<Item = xilem::WindowView<Self>> + use<> {
        let window_id = self.window_id;
        let title = self.window_title();
        let root = self.view();
        let initial_window_state = self.initial_window_state;
        std::iter::once(window(window_id, title, root).with_options(|options| {
            let options = options
                .with_decorations(false)
                .with_min_inner_size(LogicalSize::new(
                    f64::from(MIN_VIEWER_INNER_WIDTH),
                    f64::from(MIN_VIEWER_INNER_HEIGHT),
                ))
                .with_initial_inner_size(initial_window_state.inner_size());
            let options = if let Some(position) = initial_window_state.outer_position() {
                options.with_initial_position(position)
            } else {
                options
            };
            options.on_close(|state: &mut PreviewState| {
                state.running = false;
            })
        }))
    }

    fn window_title(&self) -> String {
        let base = viewer_window_title(self.document_title.as_deref());
        match self.connection.title_suffix() {
            Some(suffix) => format!("{base} - {suffix}"),
            None => base,
        }
    }

    fn status_scene(&mut self, message: &str, width: f64, height: f64) -> (Arc<Scene>, Size) {
        if let Some(scene) = &self.status_scene
            && scene.matches(message, width, height)
        {
            return (scene.scene.clone(), scene.size);
        }

        let (scene, size) = match render_status_scene(message, width, height) {
            Ok(scene) => scene,
            Err(err) => {
                log::warn!("failed to render websocket status with Typst: {err}");
                (Arc::new(Scene::new()), Size::new(width, height))
            }
        };
        self.status_scene = Some(StatusScene {
            message: message.to_owned(),
            width,
            height,
            scene,
            size,
        });

        let scene = self.status_scene.as_ref().expect("status scene just set");
        (scene.scene.clone(), scene.size)
    }

    fn apply_zoom(&mut self, action: ZoomAction) {
        self.zoom_scale = zoom_scale_after_action(self.zoom_scale, action);
    }

    fn handle_title_bar_action(&mut self, action: TitleBarAction) {
        match action {
            TitleBarAction::ToggleHelp => {
                self.help_overlay_visible = !self.help_overlay_visible;
            }
        }
    }

    fn help_overlay_scene(&mut self, width: f64, height: f64) -> (Arc<Scene>, Size) {
        if let Some(scene) = &self.help_overlay_scene
            && scene.matches(width, height)
        {
            return (scene.scene.clone(), scene.size);
        }

        let (scene, size) = match render_help_overlay_scene(width, height) {
            Ok(scene) => scene,
            Err(err) => {
                log::warn!("failed to render help overlay with Typst: {err}");
                (Arc::new(Scene::new()), Size::new(width, height))
            }
        };
        self.help_overlay_scene = Some(HelpOverlayScene {
            width,
            height,
            scene,
            size,
        });

        let scene = self
            .help_overlay_scene
            .as_ref()
            .expect("help overlay scene just set");
        (scene.scene.clone(), scene.size)
    }

    fn view(&mut self) -> impl WidgetView<Edit<Self>> + use<> {
        // Uses an effect that connects to the websocket and receives the document data.
        let effect = task(
            |proxy, args: &mut PreviewState| {
                let address = websocket_address(&args.data_plane_host);
                let rx = args.rx.take();
                let mut window_state_rx = args.window_state_rx.take();
                async move {
                    let Some(mut rx) = rx else {
                        log::warn!("spawn client multiple times for preview");
                        return;
                    };

                    send_connection_status(&proxy, ConnectionStatus::Connecting);

                    let config = ClientConfig::new(address.as_str())
                        .header("Origin", "http://localhost:23625")
                        .reconnect_interval(RECONNECT_INTERVAL);
                    let client_proxy = proxy.clone();
                    let (handle, future) = ezsockets::connect(
                        |client| Client {
                            proxy: client_proxy,
                            client,
                            doc: IncrDocClient::default(),
                            vello: IncrVelloDocClient::default(),
                            connect_attempts: 0,
                        },
                        config,
                    )
                    .await;
                    let mut future = std::pin::pin!(future);

                    loop {
                        tokio::select! {
                            event = rx.recv() => {
                                let Some(event) = event else {
                                    break;
                                };
                                match event {
                                    PreviewEvent::Click { page_idx, x, y } => {
                                        log::debug!("client click: [{page_idx}] {x}, {y}");
                                        let frame_loc = DocumentPosition {
                                            page_no: page_idx,
                                            x,
                                            y,
                                        };
                                        let frame_loc = match serde_json::to_string(&frame_loc) {
                                            Ok(frame_loc) => frame_loc,
                                            Err(err) => {
                                                log::error!("Error serializing frame location: {err}");
                                                return;
                                            }
                                        };

                                        if let Err(err) = handle.text(format!("src-point {frame_loc}")) {
                                            log::warn!("Error sending click position to websocket: {err}");
                                        }
                                    }
                                }
                            }
                            state = async {
                                match &mut window_state_rx {
                                    Some(rx) => rx.recv().await,
                                    None => None,
                                }
                            }, if window_state_rx.is_some() => {
                                let Some(state) = state else {
                                    window_state_rx = None;
                                    continue;
                                };
                                let payload = match viewer_window_state_payload(state) {
                                    Ok(payload) => payload,
                                    Err(err) => {
                                        log::debug!("failed to serialize viewer window state: {err}");
                                        continue;
                                    }
                                };
                                if let Err(err) = handle.text(format!("viewer-window-state {payload}")) {
                                    log::debug!("failed to send viewer window state to preview server: {err}");
                                }
                            }
                            res = &mut future => {
                                match res {
                                    Ok(()) => {
                                        log::warn!("websocket client stopped");
                                        send_connection_status(
                                            &proxy,
                                            ConnectionStatus::Stopped {
                                                reason: "WebSocket client stopped".into(),
                                            },
                                        );
                                    }
                                    Err(err) => {
                                        let reason = truncate_message(err.to_string(), 180);
                                        log::error!("Error connecting to websocket: {reason}");
                                        send_connection_status(
                                            &proxy,
                                            ConnectionStatus::Stopped {
                                                reason: format!("WebSocket client stopped: {reason}"),
                                            },
                                        );
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
            },
            |arg: &mut PreviewState, req: RenderRequest| {
                // s.tick();
                match req {
                    RenderRequest::New {
                        pages,
                        background_color,
                    } => {
                        arg.pages = pages;
                        arg.background_color = background_color;
                    }
                    RenderRequest::Connection(connection) => {
                        arg.connection = connection;
                    }
                }
            },
        );

        // todo: fill background size
        // , canvas: &dyn CanvasDevice, ts: sk::Transform
        // let pg = &self.pages[idx];
        // canvas.set_fill_style_str(self.fill.as_ref());
        // canvas.fill_rect(0., 0., pg.size.x.0 as f64, pg.size.y.0 as f64);

        let page_list = self
            .pages
            .iter()
            .enumerate()
            .map(|(idx, page)| {
                let tx = self.tx.clone();
                let page_scene = page.scene.clone();
                let page_accessibility = Arc::new(page.accessibility.clone());
                let background_color = self.background_color;
                let width = page.size.width;
                let height = page.size.height;

                // Adjusts size
                // This is a hack to hide the vertical scrollbar.
                // todo: hide vertical scrollbar
                let fitted_width = fitted_page_width(self.window_size.width);
                let elem_scale = page_scale(fitted_width, width, self.zoom_scale);
                let elem_width = if width > 0.0 {
                    width * elem_scale
                } else {
                    fitted_width
                };
                let elem_height = elem_scale * height;
                // The sized box is necessary to avoid collapsing the canvas.
                sized_box(
                    doc(
                        page_scene,
                        elem_scale,
                        background_color,
                        {
                            let page_accessibility = page_accessibility.clone();
                            move |pos, bbox| {
                                if bbox.width() == 0. || bbox.height() == 0. {
                                    return;
                                }

                                let x = pos.x / bbox.width() * width;
                                let y = pos.y / bbox.height() * height;

                                if let Some(href) =
                                    page_accessibility.hit_test_link(Point::new(x, y))
                                    && open_supported_external_link(href)
                                {
                                    return;
                                }

                                let _ = tx.send(PreviewEvent::Click {
                                    page_idx: idx + 1,
                                    x: x as f32,
                                    y: y as f32,
                                });
                            }
                        },
                        |state: &mut PreviewState, action| state.apply_zoom(action),
                    )
                    .accessibility(page_accessibility),
                )
                .fixed_width(Length::const_px(elem_width))
                .fixed_height(Length::const_px(elem_height))
            })
            .collect::<Vec<_>>();
        let status_overlay = if let Some(message) =
            self.connection.status_message(&self.data_plane_host)
        {
            let color = self.connection.status_color().unwrap_or(Color::TRANSPARENT);
            let overlay_width = self.window_size.width.max(1.0).ceil();
            let overlay_height = if self.pages.is_empty() {
                EMPTY_STATUS_BANNER_HEIGHT
            } else {
                STATUS_BANNER_HEIGHT
            };
            let (scene, scene_size) = self.status_scene(&message, overlay_width, overlay_height);
            let scene_scale = if scene_size.width > 0. {
                overlay_width / scene_size.width
            } else {
                1.0
            };

            Some(
                sized_box(
                    doc(
                        scene,
                        scene_scale,
                        Some(color),
                        |_pos, _bbox| {},
                        |state: &mut PreviewState, action| state.apply_zoom(action),
                    )
                    .alt_text(message),
                )
                .fixed_width(Length::const_px(overlay_width))
                .fixed_height(Length::const_px(overlay_height))
                .alignment(if self.pages.is_empty() {
                    UnitPoint::CENTER
                } else {
                    UnitPoint::TOP
                }),
            )
        } else {
            None
        };
        let help_overlay = if self.help_overlay_visible {
            let overlay_width = self.window_size.width.max(1.0).ceil();
            let overlay_height = self.window_size.height.max(1.0).ceil();
            let (scene, scene_size) = self.help_overlay_scene(overlay_width, overlay_height);
            let scene_scale = if scene_size.width > 0.0 {
                overlay_width / scene_size.width
            } else {
                1.0
            };

            Some(
                sized_box(
                    doc(
                        scene,
                        scene_scale,
                        None,
                        |_pos, _bbox| {},
                        |state: &mut PreviewState, action| state.apply_zoom(action),
                    )
                    .alt_text("Tinymist preview key bindings"),
                )
                .fixed_width(Length::const_px(overlay_width))
                .fixed_height(Length::const_px(overlay_height))
                .alignment(UnitPoint::CENTER),
            )
        } else {
            None
        };

        let preview_content = resize_observer(
            |state: &mut PreviewState, size: Size| {
                state.window_size = size;
            },
            // Adds a scrollable viewport.
            fork(
                zstack((
                    zoom_portal(
                        flex_col(page_list),
                        self.zoom_scale,
                        |state: &mut PreviewState, action| state.apply_zoom(action),
                    ),
                    status_overlay,
                    help_overlay,
                )),
                effect,
            ),
        );

        flex_col((
            title_bar(self.window_title(), |state: &mut PreviewState, action| {
                state.handle_title_bar_action(action)
            }),
            preview_content.flex(1.0),
        ))
        .gap(Gap::ZERO)
    }
}

struct StatusScene {
    message: String,
    width: f64,
    height: f64,
    scene: Arc<Scene>,
    size: Size,
}

impl StatusScene {
    fn matches(&self, message: &str, width: f64, height: f64) -> bool {
        self.message == message && self.width == width && self.height == height
    }
}

struct HelpOverlayScene {
    width: f64,
    height: f64,
    scene: Arc<Scene>,
    size: Size,
}

impl HelpOverlayScene {
    fn matches(&self, width: f64, height: f64) -> bool {
        self.width == width && self.height == height
    }
}

#[derive(Debug, Clone)]
enum ConnectionStatus {
    Connecting,
    Connected,
    Reconnecting { reason: String },
    WaitingForRetry { attempt: usize, reason: String },
    Stopped { reason: String },
}

impl ConnectionStatus {
    fn status_message(&self, data_plane_host: &str) -> Option<String> {
        match self {
            Self::Connecting => {
                let address = truncate_message(websocket_address(data_plane_host), 80);
                Some(format!("Connecting to preview server at {address}..."))
            }
            Self::Connected => None,
            Self::Reconnecting { reason } => Some(format!("{reason}. Reconnecting...")),
            Self::WaitingForRetry { attempt, reason } => Some(format!(
                "{reason}. Retrying in {}s (attempt {attempt})...",
                RECONNECT_INTERVAL.as_secs()
            )),
            Self::Stopped { reason } => Some(reason.clone()),
        }
    }

    fn title_suffix(&self) -> Option<String> {
        match self {
            Self::Connecting => Some("Connecting".into()),
            Self::Connected => None,
            Self::Reconnecting { reason } => Some(format!(
                "Reconnecting - {}",
                truncate_message(reason.clone(), 60)
            )),
            Self::WaitingForRetry { attempt, reason } => Some(format!(
                "Retrying ({attempt}) - {}",
                truncate_message(reason.clone(), 60)
            )),
            Self::Stopped { reason } => Some(format!(
                "Disconnected - {}",
                truncate_message(reason.clone(), 60)
            )),
        }
    }

    fn status_color(&self) -> Option<Color> {
        match self {
            Self::Connecting => Some(Color::from_rgba8(0x2f, 0x80, 0xed, 0xe6)),
            Self::Connected => None,
            Self::Reconnecting { .. } | Self::WaitingForRetry { .. } => {
                Some(Color::from_rgba8(0xb4, 0x53, 0x09, 0xe6))
            }
            Self::Stopped { .. } => Some(Color::from_rgba8(0xb9, 0x1c, 0x1c, 0xe6)),
        }
    }
}

fn render_status_scene(message: &str, width: f64, height: f64) -> Result<(Arc<Scene>, Size)> {
    let source = Source::new(
        FileId::new(None, VirtualPath::new("/tinymist-viewer-status.typ")),
        status_typst_source(message, width, height),
    );
    render_typst_scene(source, "status")
}

fn render_help_overlay_scene(width: f64, height: f64) -> Result<(Arc<Scene>, Size)> {
    let source = Source::new(
        FileId::new(None, VirtualPath::new("/tinymist-viewer-help-overlay.typ")),
        help_overlay_typst_source(width, height),
    );
    render_typst_scene(source, "help overlay")
}

fn render_typst_scene(source: Source, scene_name: &str) -> Result<(Arc<Scene>, Size)> {
    let world = StatusWorld { main: source };
    let compiled = typst::compile::<TypstPagedDocument>(&world);
    for warning in compiled.warnings {
        log::debug!("Typst {scene_name} render warning: {warning:?}");
    }
    let doc = compiled
        .output
        .map_err(|errors| anyhow!("failed to compile {scene_name}: {errors:?}"))?;
    let document = TypstDocument::Paged(Arc::new(doc));
    let mut renderer = IncrSvgDocServer::default();
    let frame = renderer.pack_delta(&document);
    let update = preview_update_from_bytes(&frame)
        .with_context(|| format!("{scene_name} preview frame is invalid"))?;

    let mut doc = IncrDocClient::default();
    let mut vello = IncrVelloDocClient::default();
    if update.reset_before_merge {
        doc = IncrDocClient::default();
        vello.reset();
    }
    let delta = BytesModuleStream::from_slice(update.payload).checkout_owned();
    doc.merge_delta(delta);

    let mut pages = vello.render_pages(&mut doc)?;
    pages
        .pop()
        .with_context(|| format!("Typst {scene_name} render produced no pages"))
}

fn status_typst_source(message: &str, width: f64, height: f64) -> String {
    let message = typst_string_literal(message);
    format!(
        r#"#set page(width: {width}pt, height: {height}pt, margin: 0pt)
#set text(font: "New Computer Modern", size: 10pt, fill: white)
#place(center + horizon, text({message}))
"#
    )
}

fn help_overlay_typst_source(width: f64, height: f64) -> String {
    let panel_width = help_overlay_panel_width(width);
    let key_column_width = (panel_width * 0.4)
        .clamp(190.0, 270.0)
        .min(panel_width * 0.48);

    format!(
        r##"#set page(width: {width}pt, height: {height}pt, margin: 0pt)
#set text(font: "New Computer Modern", size: 21pt, fill: rgb("#d9d9d9"))
#let key(body) = box(
  fill: rgb("#3a3a3a"),
  stroke: 0.65pt + rgb("#626262"),
  radius: 4pt,
  inset: (x: 11pt, y: 5pt),
  text(size: 18pt, fill: rgb("#f7f7f7"), body),
)
#let sep = text(fill: rgb("#858585"))[/]
#let desc(body) = text(size: 21pt, fill: rgb("#d9d9d9"), body)

#place(center + horizon)[
  #block(
    width: {panel_width}pt,
    fill: rgb("#252525"),
    stroke: 0.8pt + rgb("#505050"),
    radius: 8pt,
    inset: (x: 32pt, y: 30pt),
  )[
    #text(size: 27pt, weight: "semibold", fill: rgb("#ffffff"))[Key Bindings]
    #v(20pt)
    #grid(
      columns: ({key_column_width}pt, 1fr),
      column-gutter: 28pt,
      row-gutter: 13pt,
      align: (right + horizon, left + horizon),
      [#key[Ctrl/Cmd] #key[Wheel]],
      desc[Zoom around cursor],
      [#key[Ctrl/Cmd] #key[+] #sep #key[-]],
      desc[Zoom in or out],
      [#key[Ctrl/Cmd] #key[0]],
      desc[Reset zoom],
      [#key[Arrow keys]],
      desc[Scroll by line],
      [#key[Page Up] #sep #key[Page Down]],
      desc[Scroll by page],
      [#key[Home] #sep #key[End]],
      desc[Jump to document edges],
      [#key[Click page]],
      desc[Sync source position],
    )
  ]
]
"##
    )
}

fn help_overlay_panel_width(width: f64) -> f64 {
    let available = (width - 64.0).max(1.0);
    available.clamp(420.0, 760.0).min(width.max(1.0))
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

struct StatusWorld {
    main: Source,
}

impl World for StatusWorld {
    fn library(&self) -> &LazyHash<Library> {
        &status_typst_base().library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &status_typst_base().book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotFound(id.vpath().as_rooted_path().to_owned()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<TypstBytes> {
        Err(FileError::NotFound(id.vpath().as_rooted_path().to_owned()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        status_typst_base().fonts.get(index).cloned()
    }

    fn today(&self, _: Option<TypstDuration>) -> Option<Datetime> {
        Some(Datetime::from_ymd(1970, 1, 1).expect("valid deterministic date"))
    }
}

struct StatusTypstBase {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

fn status_typst_base() -> &'static StatusTypstBase {
    static BASE: OnceLock<StatusTypstBase> = OnceLock::new();
    BASE.get_or_init(|| {
        let fonts = typst_assets::fonts()
            .flat_map(|data| Font::iter(TypstBytes::new(data)))
            .collect::<Vec<_>>();

        StatusTypstBase {
            library: LazyHash::new(Library::builder().build()),
            book: LazyHash::new(FontBook::from_fonts(&fonts)),
            fonts,
        }
    })
}

fn websocket_address(data_plane_host: &str) -> String {
    if data_plane_host.starts_with("ws://") || data_plane_host.starts_with("wss://") {
        data_plane_host.to_owned()
    } else {
        format!("ws://{data_plane_host}")
    }
}

fn normalize_document_title(title: Option<String>) -> Option<String> {
    title
        .map(|title| title.trim().to_owned())
        .filter(|title| !title.is_empty())
}

fn viewer_window_title(document_title: Option<&str>) -> String {
    match document_title {
        Some(title) if !title.trim().is_empty() => format!("{} - Tinymist View", title.trim()),
        _ => "Tinymist View".to_owned(),
    }
}

fn send_connection_status(proxy: &MessageProxy<RenderRequest>, connection: ConnectionStatus) {
    if let Err(err) = proxy.message(RenderRequest::Connection(connection)) {
        log::debug!("failed to send websocket status to viewer: {err}");
    }
}

fn truncate_message(message: String, max_chars: usize) -> String {
    let mut chars = message.chars();
    let truncated = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        message
    }
}

fn fitted_page_width(window_width: f64) -> f64 {
    if window_width.is_finite() {
        window_width.max(1.0)
    } else {
        1.0
    }
}

fn page_scale(fitted_width: f64, page_width: f64, zoom_scale: f64) -> f64 {
    let fit_scale = if page_width > 0.0 && fitted_width.is_finite() {
        fitted_width / page_width
    } else {
        1.0
    };

    fit_scale * clamp_zoom_scale(zoom_scale)
}

fn clamp_zoom_scale(scale: f64) -> f64 {
    if scale.is_finite() {
        scale.clamp(MIN_ZOOM_SCALE, MAX_ZOOM_SCALE)
    } else {
        DEFAULT_ZOOM_SCALE
    }
}

fn zoom_scale_after_action(current: f64, action: ZoomAction) -> f64 {
    let current = clamp_zoom_scale(current);
    match action {
        ZoomAction::In => next_zoom_factor(current),
        ZoomAction::Out => previous_zoom_factor(current),
        ZoomAction::Reset => DEFAULT_ZOOM_SCALE,
    }
}

fn next_zoom_factor(current: f64) -> f64 {
    let current = clamp_zoom_scale(current);
    ZOOM_FACTORS
        .iter()
        .copied()
        .find(|factor| *factor > current + ZOOM_FACTOR_EPSILON)
        .unwrap_or(MAX_ZOOM_SCALE)
}

fn previous_zoom_factor(current: f64) -> f64 {
    let current = clamp_zoom_scale(current);
    ZOOM_FACTORS
        .iter()
        .rev()
        .copied()
        .find(|factor| *factor < current - ZOOM_FACTOR_EPSILON)
        .unwrap_or(MIN_ZOOM_SCALE)
}

enum PreviewEvent {
    Click { page_idx: usize, x: f32, y: f32 },
}

struct Client {
    proxy: MessageProxy<RenderRequest>,
    client: ezsockets::Client<Self>,
    doc: IncrDocClient,
    vello: IncrVelloDocClient,
    connect_attempts: usize,
}

#[async_trait::async_trait]
impl ezsockets::ClientExt for Client {
    type Call = ();

    async fn on_text(&mut self, text: ezsockets::Utf8Bytes) -> Result<(), ezsockets::Error> {
        log::info!("received message: {text}");
        Ok(())
    }

    async fn on_binary(&mut self, bytes: ezsockets::Bytes) -> Result<(), ezsockets::Error> {
        if let Some(update) = preview_update_from_bytes(bytes.as_ref()) {
            if update.reset_before_merge {
                self.doc = IncrDocClient::default();
                self.vello.reset();
            }
            // todo: cloned on unaligned data.
            let delta = BytesModuleStream::from_slice(update.payload).checkout_owned();

            self.doc.merge_delta(delta);

            let pages = match self.vello.render_pages_with_accessibility(&mut self.doc) {
                Ok(scene) => scene,
                Err(err) => {
                    log::error!("Error rendering pages: {err}");
                    return Ok(());
                }
            };
            let background_color = self.vello.background_color();

            let _ = self.proxy.message(RenderRequest::New {
                pages,
                background_color,
            });
        } else {
            log::info!("received bytes: {bytes:?}");
        }

        Ok(())
    }

    async fn on_connect(&mut self) -> Result<(), ezsockets::Error> {
        log::info!("connected to websocket");
        self.connect_attempts = 0;
        self.doc = IncrDocClient::default();
        self.vello.reset();
        send_connection_status(&self.proxy, ConnectionStatus::Connected);

        let res = self.client.text("current");
        if let Err(err) = res {
            log::error!("Error sending message to websocket: {err}");
        }
        Ok(())
    }

    async fn on_connect_fail(
        &mut self,
        error: ezsockets::WSError,
    ) -> Result<ClientCloseMode, ezsockets::Error> {
        self.connect_attempts += 1;
        let reason = truncate_message(format!("WebSocket connection failed: {error}"), 120);
        log::warn!("{reason}");
        send_connection_status(
            &self.proxy,
            ConnectionStatus::WaitingForRetry {
                attempt: self.connect_attempts,
                reason,
            },
        );
        Ok(ClientCloseMode::Reconnect)
    }

    async fn on_close(
        &mut self,
        frame: Option<ezsockets::CloseFrame>,
    ) -> Result<ClientCloseMode, ezsockets::Error> {
        let reason = truncate_message(
            match frame {
                Some(frame) if frame.reason.is_empty() => {
                    format!("WebSocket closed by server ({:?})", frame.code)
                }
                Some(frame) => format!(
                    "WebSocket closed by server ({:?}: {})",
                    frame.code, frame.reason
                ),
                None => "WebSocket closed by server".into(),
            },
            120,
        );
        log::warn!("{reason}");
        send_connection_status(&self.proxy, ConnectionStatus::Reconnecting { reason });
        Ok(ClientCloseMode::Reconnect)
    }

    async fn on_disconnect(&mut self) -> Result<ClientCloseMode, ezsockets::Error> {
        let reason = "WebSocket disconnected".to_owned();
        log::warn!("{reason}");
        send_connection_status(&self.proxy, ConnectionStatus::Reconnecting { reason });
        Ok(ClientCloseMode::Reconnect)
    }

    async fn on_call(&mut self, call: Self::Call) -> Result<(), ezsockets::Error> {
        let () = call;
        Ok(())
    }
}

enum RenderRequest {
    New {
        pages: Vec<RenderedPage>,
        background_color: Option<Color>,
    },
    Connection(ConnectionStatus),
}

impl fmt::Debug for RenderRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::New { pages, .. } => write!(f, "New(pages = {:?})", pages.len()),
            Self::Connection(connection) => write!(f, "Connection({connection:?})"),
        }
    }
}

fn open_supported_external_link(href: &str) -> bool {
    let href = href.trim();
    if !is_supported_external_link(href) {
        return false;
    }

    log::debug!("opening external link: {href}");
    if let Err(err) = open::that_detached(href) {
        log::warn!("failed to open external link {href:?}: {err}");
    }
    true
}

fn is_supported_external_link(href: &str) -> bool {
    let Some((scheme, rest)) = href.trim().split_once(':') else {
        return false;
    };

    if scheme.eq_ignore_ascii_case("mailto") {
        return !rest.is_empty();
    }

    (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
        && rest.starts_with("//")
        && rest.len() > 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_scale_composes_fit_width_and_zoom() {
        assert_eq!(page_scale(400.0, 200.0, 1.5), 3.0);
    }

    #[test]
    fn page_scale_uses_default_zoom_for_invalid_values() {
        assert_eq!(page_scale(400.0, 200.0, f64::NAN), 2.0);
    }

    #[test]
    fn fitted_page_width_uses_full_finite_width() {
        assert_eq!(fitted_page_width(400.5), 400.5);
        assert_eq!(fitted_page_width(0.25), 1.0);
    }

    #[test]
    fn fitted_page_width_uses_safe_width_for_invalid_values() {
        assert_eq!(fitted_page_width(f64::NAN), 1.0);
        assert_eq!(fitted_page_width(f64::INFINITY), 1.0);
    }

    #[test]
    fn zoom_actions_update_and_reset_scale() {
        assert_eq!(zoom_scale_after_action(1.0, ZoomAction::In), 1.1);
        assert_eq!(zoom_scale_after_action(1.1, ZoomAction::In), 1.3);
        assert_eq!(zoom_scale_after_action(1.0, ZoomAction::Out), 0.9);
        assert_eq!(zoom_scale_after_action(1.1, ZoomAction::Out), 1.0);
        assert_eq!(
            zoom_scale_after_action(2.0, ZoomAction::Reset),
            DEFAULT_ZOOM_SCALE
        );
    }

    #[test]
    fn zoom_actions_snap_from_in_between_values_to_next_ladder_factor() {
        assert_eq!(zoom_scale_after_action(1.2, ZoomAction::In), 1.3);
        assert_eq!(zoom_scale_after_action(1.2, ZoomAction::Out), 1.1);
    }

    #[test]
    fn zoom_scale_is_clamped_to_supported_range() {
        assert_eq!(
            zoom_scale_after_action(MAX_ZOOM_SCALE, ZoomAction::In),
            MAX_ZOOM_SCALE
        );
        assert_eq!(
            zoom_scale_after_action(MIN_ZOOM_SCALE, ZoomAction::Out),
            MIN_ZOOM_SCALE
        );
    }

    #[test]
    fn supported_external_links_are_limited_to_web_and_mail() {
        assert!(is_supported_external_link("http://example.com"));
        assert!(is_supported_external_link("https://example.com/path"));
        assert!(is_supported_external_link("mailto:hello@example.com"));
        assert!(is_supported_external_link("HTTPS://example.com"));
    }

    #[test]
    fn unsupported_external_links_are_not_opened_by_viewer() {
        assert!(!is_supported_external_link("file:///tmp/example.typ"));
        assert!(!is_supported_external_link("/tmp/example.typ"));
        assert!(!is_supported_external_link("example.typ"));
        assert!(!is_supported_external_link("#target"));
        assert!(!is_supported_external_link("ftp://example.com"));
        assert!(!is_supported_external_link("http:example.com"));
        assert!(!is_supported_external_link("mailto:"));
    }

    #[test]
    fn viewer_window_state_ignores_transient_small_sizes() {
        let mut state = ViewerWindowState::default();

        assert!(!state.set_inner_size(PhysicalSize::new(320, 200)));
        assert_eq!(
            state.inner_size(),
            PhysicalSize::new(MIN_VIEWER_INNER_WIDTH, MIN_VIEWER_INNER_HEIGHT)
        );

        assert!(state.set_inner_size(PhysicalSize::new(1024, 900)));
        assert_eq!(state.inner_size(), PhysicalSize::new(1024, 900));
    }

    #[test]
    fn initial_window_state_uses_cli_geometry() {
        let state = ViewerWindowState::from_initial_geometry(
            Some(PhysicalSize::new(1280, 720)),
            Some(PhysicalPosition::new(-12, 34)),
        );

        assert_eq!(state.inner_size(), PhysicalSize::new(1280, 720));
        assert_eq!(state.outer_position(), Some(PhysicalPosition::new(-12, 34)));
    }

    #[test]
    fn parses_initial_window_inner_size() {
        assert_eq!(
            parse_initial_window_inner_size("1280x900").expect("size should parse"),
            PhysicalSize::new(1280, 900)
        );
        assert_eq!(
            parse_initial_window_inner_size("1280X900").expect("size should parse"),
            PhysicalSize::new(1280, 900)
        );

        assert!(parse_initial_window_inner_size("1280,900").is_err());
        assert!(parse_initial_window_inner_size("0x900").is_err());
        assert!(parse_initial_window_inner_size("1280x0").is_err());
        assert!(parse_initial_window_inner_size("wide").is_err());
    }

    #[test]
    fn parses_initial_window_position() {
        assert_eq!(
            parse_initial_window_position("-10,20").expect("position should parse"),
            PhysicalPosition::new(-10, 20)
        );
        assert_eq!(
            parse_initial_window_position(" 10 , -20 ").expect("position should parse"),
            PhysicalPosition::new(10, -20)
        );

        assert!(parse_initial_window_position("10x20").is_err());
        assert!(parse_initial_window_position("left,20").is_err());
    }

    #[test]
    fn cli_accepts_negative_initial_window_position() {
        let args =
            Args::try_parse_from(["tinymist-viewer", "--initial-window-position", "-541,349"])
                .expect("negative window position should parse as an option value");

        assert_eq!(
            args.initial_window_position,
            Some(PhysicalPosition::new(-541, 349))
        );
    }

    #[test]
    fn viewer_window_state_payload_uses_schema_payload() {
        let message = viewer_window_state_payload(ViewerWindowState {
            inner_width: 1280,
            inner_height: 900,
            outer_x: Some(24),
            outer_y: Some(48),
        })
        .expect("state should serialize");

        let payload = serde_json::from_str::<serde_json::Value>(&message)
            .expect("event payload should be valid json");

        assert_eq!(
            payload["schema_version"],
            VIEWER_WINDOW_STATE_SCHEMA_VERSION
        );
        assert_eq!(payload["window"]["inner_width"], 1280);
        assert_eq!(payload["window"]["inner_height"], 900);
        assert_eq!(payload["window"]["outer_x"], 24);
        assert_eq!(payload["window"]["outer_y"], 48);
    }

    #[test]
    fn viewer_window_title_uses_document_title_when_available() {
        assert_eq!(
            viewer_window_title(Some("main.typ")),
            "main.typ - Tinymist View"
        );
        assert_eq!(viewer_window_title(Some("  ")), "Tinymist View");
        assert_eq!(viewer_window_title(None), "Tinymist View");
    }

    #[test]
    fn typst_status_scene_renders() {
        let (scene, size) = render_status_scene("Retrying websocket connection", 320.0, 28.0)
            .expect("status text should render through Typst");

        assert_eq!(size, Size::new(320.0, 28.0));
        assert!(
            !scene.encoding().is_empty(),
            "status scene should contain glyphs"
        );
    }

    #[test]
    fn typst_help_overlay_scene_renders() {
        let (scene, size) =
            render_help_overlay_scene(800.0, 600.0).expect("help overlay should render");

        assert_eq!(size, Size::new(800.0, 600.0));
        assert!(
            !scene.encoding().is_empty(),
            "help overlay scene should contain panel content"
        );
    }
}
