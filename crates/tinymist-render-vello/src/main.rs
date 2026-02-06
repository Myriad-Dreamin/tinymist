//! Connects to tinymist language server and renders the document to a vello

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use ezsockets::ClientConfig;
use reflexo::vector::incr::IncrDocClient;
use reflexo::vector::stream::BytesModuleStream;
use vello::peniko::color::palette;
use vello::util::{RenderContext, RenderSurface};
use vello::wgpu;
use vello::{AaConfig, Renderer, RendererOptions, Scene};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::Window;

use tinymist_render_vello::incr::IncrVelloDocClient;

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

    let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    // Sets up a bunch of state:
    let mut app = SimpleVelloApp {
        context: RenderContext::new(),
        renderers: vec![],
        state: RenderState::Suspended(None),
        scene: Scene::new(),
    };

    // Creates and run a winit event loop
    let event_loop = EventLoop::<RenderRequest>::with_user_event().build()?;

    let proxy = event_loop.create_proxy();

    tokio_runtime.spawn(async move {
        let address = if args.data_plane_host.contains("ws://") {
            args.data_plane_host.clone()
        } else {
            format!("ws://{}", args.data_plane_host)
        };
        let config = ClientConfig::new(address.as_str()).header("Origin", "http://localhost:23625");
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
    });

    event_loop
        .run_app(&mut app)
        .context("Couldn't run event loop")?;

    Ok(())
}

#[derive(Debug)]
enum RenderState {
    /// `RenderSurface` and `Window` for active rendering.
    Active {
        surface: Box<RenderSurface<'static>>,
        valid_surface: bool,
        window: Arc<Window>,
    },
    /// Cache a window so that it can be reused when the app is resumed after
    /// being suspended.
    Suspended(Option<Arc<Window>>),
}

struct SimpleVelloApp {
    /// The Vello `RenderContext` which is a global context that lasts for the
    /// lifetime of the application
    context: RenderContext,

    /// An array of renderers, one per wgpu device
    renderers: Vec<Option<Renderer>>,

    /// State for our example where we store the winit Window and the wgpu
    /// Surface
    state: RenderState,

    /// A vello Scene which is a data structure which allows one to build up a
    /// description a scene to be drawn (with paths, fills, images, text, etc)
    /// which is then passed to a renderer for rendering
    scene: Scene,
}

impl ApplicationHandler<RenderRequest> for SimpleVelloApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let RenderState::Suspended(cached_window) = &mut self.state else {
            return;
        };

        // Get the winit window cached in a previous Suspended event or else create a
        // new window
        let window = cached_window
            .take()
            .unwrap_or_else(|| create_winit_window(event_loop));

        // Create a vello Surface
        let size = window.inner_size();
        let surface_future = self.context.create_surface(
            window.clone(),
            size.width,
            size.height,
            wgpu::PresentMode::AutoVsync,
        );
        let surface = pollster::block_on(surface_future).expect("Error creating surface");

        // Create a vello Renderer for the surface (using its device id)
        self.renderers
            .resize_with(self.context.devices.len(), || None);
        self.renderers[surface.dev_id]
            .get_or_insert_with(|| create_vello_renderer(&self.context, &surface));

        // Save the Window and Surface to a state variable
        self.state = RenderState::Active {
            surface: Box::new(surface),
            valid_surface: true,
            window,
        };
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        if let RenderState::Active { window, .. } = &self.state {
            self.state = RenderState::Suspended(Some(window.clone()));
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: RenderRequest) {
        match event {
            RenderRequest::New(scene) => {
                self.scene = scene;
                self.render();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        // Only process events for our window, and only when we have a surface.
        let (surface, valid_surface) = match &mut self.state {
            RenderState::Active {
                surface,
                valid_surface,
                window,
            } if window.id() == window_id => (surface, valid_surface),
            _ => return,
        };

        match event {
            // Exit the event loop when a close is requested (e.g. window's close button is pressed)
            WindowEvent::CloseRequested => event_loop.exit(),

            // Resize the surface when the window is resized
            WindowEvent::Resized(size) => {
                if size.width != 0 && size.height != 0 {
                    self.context
                        .resize_surface(surface, size.width, size.height);
                    *valid_surface = true;
                } else {
                    *valid_surface = false;
                }
            }

            // This is where all the rendering happens
            WindowEvent::RedrawRequested => {
                if !*valid_surface {
                    return;
                }

                self.render();
            }
            _ => {}
        }
    }
}

impl SimpleVelloApp {
    /// Renders the scene to the surface.
    fn render(&mut self) {
        // Only process events for our window, and only when we have a surface.
        let RenderState::Active { surface, .. } = &mut self.state else {
            return;
        };

        // Get the window size
        let width = surface.config.width;
        let height = surface.config.height;

        // Get a handle to the device
        let device_handle = &self.context.devices[surface.dev_id];

        // Render to a texture, which we will later copy into the surface
        self.renderers[surface.dev_id]
            .as_mut()
            .unwrap()
            .render_to_texture(
                &device_handle.device,
                &device_handle.queue,
                &self.scene,
                &surface.target_view,
                &vello::RenderParams {
                    base_color: palette::css::WHITE, // Background color
                    width,
                    height,
                    antialiasing_method: AaConfig::Msaa16,
                },
            )
            .expect("failed to render to surface");

        // Get the surface's texture
        let surface_texture = surface
            .surface
            .get_current_texture()
            .expect("failed to get surface texture");

        // Perform the copy
        let mut encoder =
            device_handle
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Surface Blit"),
                });
        surface.blitter.copy(
            &device_handle.device,
            &mut encoder,
            &surface.target_view,
            &surface_texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
        );
        device_handle.queue.submit([encoder.finish()]);
        // Queue the texture to be presented on the surface
        surface_texture.present();

        device_handle.device.poll(wgpu::PollType::Poll).unwrap();
    }
}

struct Client {
    proxy: EventLoopProxy<RenderRequest>,
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

            let scene = match self.vello.render_pages(&mut self.doc) {
                Ok(scene) => scene,
                Err(err) => {
                    log::error!("Error rendering pages: {err}");
                    return Ok(());
                }
            };

            let _ = self.proxy.send_event(RenderRequest::New(scene));
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
    New(vello::Scene),
}

/// Helper function that creates a Winit window and returns it (wrapped in an
/// Arc for sharing between threads)
fn create_winit_window(event_loop: &ActiveEventLoop) -> Arc<Window> {
    let attr = Window::default_attributes()
        .with_inner_size(LogicalSize::new(1044, 800))
        .with_resizable(true)
        .with_title("Tinymist Preview");
    Arc::new(event_loop.create_window(attr).unwrap())
}

/// Helper function that creates a vello `Renderer` for a given `RenderContext`
/// and `RenderSurface`
fn create_vello_renderer(render_cx: &RenderContext, surface: &RenderSurface<'_>) -> Renderer {
    Renderer::new(
        &render_cx.devices[surface.dev_id].device,
        RendererOptions::default(),
    )
    .expect("Couldn't create renderer")
}
