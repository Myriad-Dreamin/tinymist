//! Renders and views typst document with Xilem & Vello.

use core::fmt;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use ezsockets::ClientConfig;
use masonry::layout::Length;
use reflexo::vector::incr::IncrDocClient;
use reflexo::vector::stream::BytesModuleStream;
use vello::kurbo::Affine;
use winit::dpi::LogicalSize;
use xilem::core::{Edit, MessageProxy, fork};
use xilem::vello::Scene;
use xilem::vello::kurbo::Size;
use xilem::view::{canvas, flex_col, portal, resize_observer, sized_box, task};
use xilem::{EventLoop, WidgetView, WindowOptions, Xilem};

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

    let default_size = Size::new(800.0, 800.0);
    let app = Xilem::new_simple(
        PreviewState {
            data_plane_host: args.data_plane_host,
            pages: vec![],
            window_size: default_size,
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
                async move {
                    let config = ClientConfig::new(address.as_str())
                        .header("Origin", "http://localhost:23625");
                    let (_handle, future) = ezsockets::connect(
                        |client| Client {
                            proxy,
                            client,
                            doc: IncrDocClient::default(),
                            vello: IncrVelloDocClient::default(),
                        },
                        config,
                    )
                    .await;

                    // todo: the client is down if we don't sleep for a while.
                    tokio::time::sleep(std::time::Duration::from_secs(600)).await;

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

        // if !set_transform(canvas, ts) {
        //     return;
        // }
        // canvas.set_fill_style_str(self.fill.as_ref());
        // canvas.fill_rect(0., 0., pg.size.x.0 as f64, pg.size.y.0 as f64);

        // pg.elem.realize(ts, canvas).await;

        let page_list = self
            .pages
            .iter()
            .map(|(page_scene, scene_size)| {
                let page_scene = page_scene.clone();
                let width = scene_size.width;
                let height = scene_size.height;

                // This is a hack to hide the vertical scrollbar.
                // todo: hide vertical scrollbar
                let elem_width = self.window_size.width - 0.5;
                let elem_scale = elem_width / width;
                let elem_height = elem_scale * height;
                // The sized box is necessary to avoid collapsing the canvas.
                sized_box(canvas(
                    move |_state: &mut Self, _ctx, scene: &mut Scene, _size: Size| {
                        log::info!("canvas size: {:?}", _size);
                        // Adjusts width
                        let scale_ts = if width > 0. {
                            Some(Affine::scale(elem_scale))
                        } else {
                            None
                        };
                        scene.append(&page_scene, scale_ts);
                    },
                ))
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

        let _ = self.proxy;
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
