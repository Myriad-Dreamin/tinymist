//! Renders and views typst document with Xilem & Vello.

use core::fmt;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use ezsockets::ClientConfig;
use masonry::layout::Length;
use reflexo::debug_loc::DocumentPosition;
use reflexo::vector::incr::IncrDocClient;
use reflexo::vector::stream::BytesModuleStream;
use tokio::sync::mpsc;
use winit::dpi::LogicalSize;
use xilem::core::{Edit, MessageProxy, fork};
use xilem::vello::Scene;
use xilem::vello::kurbo::Size;
use xilem::view::{flex_col, portal, resize_observer, sized_box, task};
use xilem::{EventLoop, WidgetView, WindowOptions, Xilem};

use tinymist_viewer::doc::doc;
use tinymist_viewer::incr::IncrVelloDocClient;

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
    let app = Xilem::new_simple(
        PreviewState {
            data_plane_host: args.data_plane_host,
            pages: vec![],
            window_size: default_size,
            tx,
            rx: Some(rx),
        },
        PreviewState::view,
        WindowOptions::new("Tinymist View").with_min_inner_size(LogicalSize::new(800.0, 800.0)),
    );
    app.run_in(EventLoop::with_user_event())
        .context("Couldn't run event loop")?;
    Ok(())
}

struct PreviewState {
    data_plane_host: String,
    pages: Vec<(Arc<Scene>, Size)>,
    window_size: Size,
    tx: mpsc::UnboundedSender<PreviewEvent>,
    rx: Option<mpsc::UnboundedReceiver<PreviewEvent>>,
}

impl PreviewState {
    fn view(&mut self) -> impl WidgetView<Edit<Self>> + use<> {
        // Uses an effect that connects to the websocket and receives the document data.
        let effect = task(
            |proxy, args: &mut PreviewState| {
                let address = if args.data_plane_host.contains("ws://") {
                    args.data_plane_host.clone()
                } else {
                    format!("ws://{}", args.data_plane_host)
                };
                let rx = args.rx.take();
                async move {
                    let Some(mut rx) = rx else {
                        log::warn!("spawn client multiple times for preview");
                        return;
                    };

                    let config = ClientConfig::new(address.as_str())
                        .header("Origin", "http://localhost:23625");
                    let (handle, future) = ezsockets::connect(
                        |client| Client {
                            proxy,
                            client,
                            doc: IncrDocClient::default(),
                            vello: IncrVelloDocClient::default(),
                        },
                        config,
                    )
                    .await;

                    while let Some(event) = rx.recv().await {
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

                                let _ = handle.text(format!("src-point {frame_loc}"));
                            }
                        }
                    }

                    let res = future.await;
                    if let Err(err) = res {
                        log::error!("Error connecting to websocket: {err}");
                    }
                }
            },
            |arg: &mut PreviewState, req: RenderRequest| {
                // s.tick();
                match req {
                    RenderRequest::New(pages) => {
                        arg.pages = pages;
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
                let width = scene_size.width;
                let height = scene_size.height;

                // Adjusts size
                // This is a hack to hide the vertical scrollbar.
                // todo: hide vertical scrollbar
                let elem_width = self.window_size.width - 0.5;
                let elem_scale = if width > 0. { elem_width / width } else { 1.0 };
                let elem_height = elem_scale * height;
                // The sized box is necessary to avoid collapsing the canvas.
                sized_box(doc(page_scene, elem_scale, move |pos, bbox| {
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
                }))
                .fixed_width(Length::const_px(elem_width))
                .fixed_height(Length::const_px(elem_height))
            })
            .collect::<Vec<_>>();

        // Listens to window size changes and renders the scene.
        resize_observer(
            |state: &mut PreviewState, size: Size| {
                state.window_size = size;
            },
            // Adds a scroll bar
            fork(portal(flex_col(page_list)), effect),
        )
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
}

#[async_trait::async_trait]
impl ezsockets::ClientExt for Client {
    type Call = ();

    async fn on_text(&mut self, text: ezsockets::Utf8Bytes) -> Result<(), ezsockets::Error> {
        log::info!("received message: {text}");
        Ok(())
    }

    async fn on_binary(&mut self, bytes: ezsockets::Bytes) -> Result<(), ezsockets::Error> {
        const DIFF_V1_PREFIX: &[u8] = b"diff-v1,";

        if bytes.starts_with(DIFF_V1_PREFIX) {
            let diff = bytes.slice(DIFF_V1_PREFIX.len()..);

            // todo: cloned on unaligned data.
            let delta = BytesModuleStream::from_slice(&diff).checkout_owned();

            self.doc.merge_delta(delta);

            let pages = match self.vello.render_pages(&mut self.doc) {
                Ok(scene) => scene,
                Err(err) => {
                    log::error!("Error rendering pages: {err}");
                    return Ok(());
                }
            };

            let _ = self.proxy.message(RenderRequest::New(pages));
        } else {
            log::info!("received bytes: {bytes:?}");
        }

        Ok(())
    }

    async fn on_connect(&mut self) -> Result<(), ezsockets::Error> {
        log::info!("connected to websocket");

        let res = self.client.text("current");
        if let Err(err) = res {
            log::error!("Error sending message to websocket: {err}");
        }
        Ok(())
    }

    async fn on_call(&mut self, call: Self::Call) -> Result<(), ezsockets::Error> {
        let () = call;
        Ok(())
    }
}

enum RenderRequest {
    New(Vec<(Arc<Scene>, Size)>),
}

impl fmt::Debug for RenderRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::New(scenes) => write!(f, "New(pages = {:?})", scenes.len()),
        }
    }
}
