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
use reflexo::debug_loc::DocumentPosition;
use reflexo::typst_shim::syntax::VirtualPathExt;
use reflexo::vector::incr::IncrDocClient;
use reflexo::vector::stream::BytesModuleStream;
use reflexo_vec2svg::IncrSvgDocServer;
use tinymist_std::typst::TypstDocument;
use tokio::sync::mpsc;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes as TypstBytes, Datetime, Duration as TypstDuration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;
use winit::dpi::LogicalSize;
use xilem::core::{Edit, MessageProxy, fork};
use xilem::vello::Scene;
use xilem::vello::kurbo::{Point, Size};
use xilem::vello::peniko::Color;
use xilem::view::{ZStackExt as _, flex_col, resize_observer, sized_box, task, zstack};
use xilem::{AppState, EventLoop, WidgetView, WindowId, Xilem, window};

use tinymist_viewer::doc::{ZoomAction, doc};
use tinymist_viewer::incr::{IncrVelloDocClient, RenderedPage};
use tinymist_viewer::protocol::preview_update_from_bytes;
use tinymist_viewer::zoom_portal::zoom_portal;

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
}

fn main() -> Result<()> {
    let args = Args::parse();

    env_logger::builder()
        .filter_module("tinymist", log::LevelFilter::Info)
        .try_init()?;

    let (tx, rx) = mpsc::unbounded_channel();

    let default_size = Size::new(800.0, 800.0);
    let data_plane_host = args.data_plane_host;
    let app = Xilem::new(
        PreviewState {
            data_plane_host,
            window_id: WindowId::next(),
            running: true,
            pages: vec![],
            background_color: None,
            window_size: default_size,
            connection: ConnectionStatus::Connecting,
            status_scene: None,
            zoom_scale: DEFAULT_ZOOM_SCALE,
            tx,
            rx: Some(rx),
        },
        PreviewState::windows,
    );
    app.run_in(EventLoop::with_user_event())
        .context("Couldn't run event loop")?;
    Ok(())
}

struct PreviewState {
    data_plane_host: String,
    window_id: WindowId,
    running: bool,
    pages: Vec<RenderedPage>,
    background_color: Option<Color>,
    window_size: Size,
    connection: ConnectionStatus,
    status_scene: Option<StatusScene>,
    zoom_scale: f64,
    tx: mpsc::UnboundedSender<PreviewEvent>,
    rx: Option<mpsc::UnboundedReceiver<PreviewEvent>>,
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
        std::iter::once(window(window_id, title, root).with_options(|options| {
            options
                .with_min_inner_size(LogicalSize::new(800.0, 800.0))
                .on_close(|state: &mut PreviewState| {
                    state.running = false;
                })
        }))
    }

    fn window_title(&self) -> String {
        match self.connection.title_suffix() {
            Some(suffix) => format!("Tinymist View - {suffix}"),
            None => "Tinymist View".to_owned(),
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

    fn view(&mut self) -> impl WidgetView<Edit<Self>> + use<> {
        // Uses an effect that connects to the websocket and receives the document data.
        let effect = task(
            |proxy, args: &mut PreviewState| {
                let address = websocket_address(&args.data_plane_host);
                let rx = args.rx.take();
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

        // Listens to window size changes and renders the scene.
        resize_observer(
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
                )),
                effect,
            ),
        )
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
        FileId::new(RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("tinymist-viewer-status.typ")
                .expect("valid tinymist viewer status path"),
        )),
        status_typst_source(message, width, height),
    );
    let world = StatusWorld { main: source };
    let compiled = typst::compile::<PagedDocument>(&world);
    for warning in compiled.warnings {
        log::debug!("Typst status render warning: {warning:?}");
    }
    let doc = compiled
        .output
        .map_err(|errors| anyhow!("failed to compile status text: {errors:?}"))?;
    let document = TypstDocument::Paged(Arc::new(doc));
    let mut renderer = IncrSvgDocServer::default();
    let frame = renderer.pack_delta(&document);
    let update = preview_update_from_bytes(&frame).context("status preview frame is invalid")?;

    let mut doc = IncrDocClient::default();
    let mut vello = IncrVelloDocClient::default();
    if update.reset_before_merge {
        doc = IncrDocClient::default();
        vello.reset();
    }
    let delta = BytesModuleStream::from_slice(update.payload).checkout_owned();
    doc.merge_delta(delta);

    let mut pages = vello.render_pages(&mut doc)?;
    pages.pop().context("Typst status render produced no pages")
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
            Err(FileError::NotFound(
                id.vpath().as_rooted_path_compat().to_path_buf(),
            ))
        }
    }

    fn file(&self, id: FileId) -> FileResult<TypstBytes> {
        Err(FileError::NotFound(
            id.vpath().as_rooted_path_compat().to_path_buf(),
        ))
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
        (window_width - 0.5).max(1.0)
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
    fn typst_status_scene_renders() {
        let (scene, size) = render_status_scene("Retrying websocket connection", 320.0, 28.0)
            .expect("status text should render through Typst");

        assert_eq!(size, Size::new(320.0, 28.0));
        assert!(
            !scene.encoding().is_empty(),
            "status scene should contain glyphs"
        );
    }
}
