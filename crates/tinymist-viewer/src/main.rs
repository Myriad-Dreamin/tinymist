//! Renders and views typst document with Xilem & Vello.

use core::fmt;

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
use xilem::view::{canvas, portal, resize_observer, sized_box, task};
use xilem::{EventLoop, WidgetView, WindowOptions, Xilem};

use tinymist_viewer::incr::IncrVelloDocClient;

struct Circles {
    data_plane_host: String,
    scene: Scene,
    scene_size: Size,
    window_size: Size,
}

impl Circles {
    fn view(&mut self) -> impl WidgetView<Edit<Self>> + use<> {
        let effect = task(
            |proxy, args: &mut Circles| {
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
            |arg: &mut Circles, req: RenderRequest| {
                // s.tick();
                match req {
                    RenderRequest::New(scene, size) => {
                        arg.scene = scene;
                        arg.scene_size = size;
                    }
                }
            },
        );

        resize_observer(
            |state: &mut Circles, size: Size| {
                state.window_size = size;
            },
            portal(fork(
                sized_box(canvas(
                    |state: &mut Self, _ctx, scene: &mut Scene, _size: Size| {
                        log::info!("canvas size: {:?}", _size);
                        let scale_ts = if state.scene_size.width > 0. {
                            Some(Affine::scale(
                                state.window_size.width / state.scene_size.width,
                            ))
                        } else {
                            None
                        };
                        scene.append(&state.scene, scale_ts);
                    },
                ))
                // todo: pt or px?
                // todo: hide vertical scrollbar
                .fixed_width(Length::const_px(self.window_size.width - 0.5))
                .fixed_height(Length::const_px(self.scene_size.height)),
                effect,
            )),
        )
    }
}

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
        Circles {
            data_plane_host: args.data_plane_host,
            scene: Scene::new(),
            scene_size: default_size,
            window_size: default_size,
        },
        Circles::view,
        WindowOptions::new("Tinymist View").with_min_inner_size(LogicalSize::new(800.0, 800.0)),
    );
    app.run_in(EventLoop::with_user_event())
        .context("Couldn't run event loop")?;
    Ok(())
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

            let (scene, size) = match self.vello.render_pages(&mut self.doc) {
                Ok(scene) => scene,
                Err(err) => {
                    log::error!("Error rendering pages: {err}");
                    return Ok(());
                }
            };

            let _ = self.proxy.message(RenderRequest::New(
                scene,
                // todo: dpi
                Size::new(size.x * 2., size.y * 2.),
            ));
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
    New(vello::Scene, Size),
}

impl fmt::Debug for RenderRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::New(.., size) => write!(f, "New({size:?})"),
        }
    }
}
