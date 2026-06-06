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
use reflexo::vector::incr::IncrDocClient;
use reflexo::vector::stream::BytesModuleStream;
use reflexo_vec2svg::IncrSvgDocServer;
use tinymist_std::typst::TypstDocument;
use tokio::sync::mpsc;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes as TypstBytes, Datetime};
use typst::layout::PagedDocument;
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use winit::dpi::LogicalSize;
use xilem::core::{Edit, MessageProxy, fork};
use xilem::vello::Scene;
use xilem::vello::kurbo::Size;
use xilem::vello::peniko::Color;
use xilem::view::{ZStackExt as _, flex_col, portal, resize_observer, sized_box, task, zstack};
use xilem::{AppState, EventLoop, WidgetView, WindowId, Xilem, window};

use tinymist_viewer::doc::doc;
use tinymist_viewer::incr::IncrVelloDocClient;
use tinymist_viewer::protocol::preview_update_from_bytes;

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
    pages: Vec<(Arc<Scene>, Size)>,
    background_color: Option<Color>,
    window_size: Size,
    connection: ConnectionStatus,
    status_scene: Option<StatusScene>,
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
            .map(|(idx, (page_scene, scene_size))| {
                let tx = self.tx.clone();
                let page_scene = page_scene.clone();
                let background_color = self.background_color;
                let width = scene_size.width;
                let height = scene_size.height;

                // Adjusts size
                // This is a hack to hide the vertical scrollbar.
                // todo: hide vertical scrollbar
                let elem_width = self.window_size.width - 0.5;
                let elem_scale = if width > 0. { elem_width / width } else { 1.0 };
                let elem_height = elem_scale * height;
                // The sized box is necessary to avoid collapsing the canvas.
                sized_box(doc(
                    page_scene,
                    elem_scale,
                    background_color,
                    move |pos, bbox| {
                        if bbox.width() == 0. || bbox.height() == 0. {
                            return;
                        }

                        let x = pos.x / bbox.width() * width;
                        let y = pos.y / bbox.height() * height;

                        let _ = tx.send(PreviewEvent::Click {
                            page_idx: idx + 1,
                            x: x as f32,
                            y: y as f32,
                        });
                    },
                ))
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
                sized_box(doc(scene, scene_scale, Some(color), |_pos, _bbox| {}).alt_text(message))
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
            // Adds a scroll bar
            fork(
                zstack((portal(flex_col(page_list)), status_overlay)),
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
        FileId::new(None, VirtualPath::new("/tinymist-viewer-status.typ")),
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
            Err(FileError::NotFound(id.vpath().as_rooted_path().to_owned()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<TypstBytes> {
        Err(FileError::NotFound(id.vpath().as_rooted_path().to_owned()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        status_typst_base().fonts.get(index).cloned()
    }

    fn today(&self, _: Option<i64>) -> Option<Datetime> {
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

            let pages = match self.vello.render_pages(&mut self.doc) {
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
        pages: Vec<(Arc<Scene>, Size)>,
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

#[cfg(test)]
mod tests {
    use super::*;

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
