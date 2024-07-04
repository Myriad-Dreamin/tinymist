use std::{borrow::Cow, collections::HashMap, net::SocketAddr, path::Path, sync::Arc};

use actor::typ_client::CompileHandler;
use anyhow::Context;
use await_tree::InstrumentAwait;
use hyper::{
    service::{make_service_fn, service_fn},
    Error,
};
use log::{error, info};
use lsp_types::notification::Notification;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sync_lsp::just_ok;
use tinymist_assets::TYPST_PREVIEW_HTML;
use tokio::sync::{mpsc, oneshot};
use typst::foundations::{Str, Value};
use typst_preview::{
    await_tree::{get_await_tree_async, REGISTRY},
    preview, ControlPlaneMessage, ControlPlaneResponse, DocToSrcJumpInfo, LspEditorConnection,
    PreviewArgs, PreviewMode, Previewer,
};
use typst_ts_core::config::{compiler::EntryOpts, CompileOpts};

use super::*;

#[derive(Debug, Clone, clap::Parser)]
pub struct PreviewCliArgs {
    #[clap(flatten)]
    pub preview: PreviewArgs,

    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// Used by lsp for identifying the task.
    #[clap(
        long = "task-id",
        default_value = "",
        value_name = "TASK_ID",
        hide(true)
    )]
    pub task_id: String,

    /// Preview mode
    #[clap(long = "preview-mode", default_value = "document", value_name = "MODE")]
    pub preview_mode: PreviewMode,

    /// Host for the preview server
    #[clap(
        long = "host",
        value_name = "HOST",
        default_value = "127.0.0.1:23627",
        alias = "static-file-host"
    )]
    pub static_file_host: String,

    /// Don't open the preview in the browser after compilation.
    #[clap(long = "no-open")]
    pub dont_open_in_browser: bool,
}

pub struct PreviewActor {
    client: TypedLspClient<PreviewState>,
    tabs: HashMap<String, PreviewTab>,
    preview_rx: mpsc::UnboundedReceiver<PreviewRequest>,
}

impl PreviewActor {
    pub async fn run(mut self) {
        while let Some(req) = self.preview_rx.recv().await {
            match req {
                PreviewRequest::Started(tab) => {
                    self.tabs.insert(tab.task_id.clone(), tab);
                }
                PreviewRequest::Kill(task_id, tx) => {
                    info!("Preview Killing: {task_id}");
                    if let Some(tab) = self.tabs.remove(&task_id) {
                        tab.previewer.stop().await;
                        let _ = tab.static_server_killer.send(());
                        let client = self.client.clone();
                        self.client.handle.spawn(async move {
                            // Wait for previewer to stop
                            tab.previewer.join().await;
                            let _ = tab.static_server_handle.await;

                            tab.cc.unregister_preview(tab.task_id);

                            info!("Preview killed: {task_id}");

                            // Send response
                            let _ = tx.send(Ok(JsonValue::Null));

                            // Send global notification
                            client.send_notification::<DisposePreview>(DisposePreview { task_id });
                        });
                    } else {
                        let _ = tx.send(Err(internal_error("task not found")));
                    }
                }
                PreviewRequest::Scroll(task_id, req) => {
                    self.scroll(task_id, req).await;
                }
            }
        }
    }

    async fn scroll(&mut self, task_id: String, req: ControlPlaneMessage) -> Option<()> {
        let task = self.tabs.get(&task_id)?;

        task.ctl_tx.send(req).ok()
    }
}

pub struct PreviewState {
    client: TypedLspClient<PreviewState>,

    preview_tx: mpsc::UnboundedSender<PreviewRequest>,
}

impl PreviewState {
    pub fn new(client: TypedLspClient<PreviewState>) -> Self {
        let (preview_tx, preview_rx) = mpsc::unbounded_channel();

        client.handle.spawn(
            PreviewActor {
                client: client.clone(),
                tabs: HashMap::default(),
                preview_rx,
            }
            .run(),
        );

        Self { client, preview_tx }
    }
}

struct PreviewTab {
    task_id: String,
    previewer: Previewer,
    static_server_killer: oneshot::Sender<()>,
    static_server_handle: tokio::task::JoinHandle<()>,

    ctl_tx: mpsc::UnboundedSender<ControlPlaneMessage>,

    cc: Arc<CompileHandler>,
}

enum PreviewRequest {
    Started(PreviewTab),
    Kill(String, oneshot::Sender<LspResult<JsonValue>>),
    Scroll(String, ControlPlaneMessage),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartPreviewResponse {
    static_server_port: Option<u16>,
    static_server_addr: Option<String>,
    data_plane_port: Option<u16>,
}

impl PreviewState {
    pub fn start(
        &self,
        mut args: PreviewCliArgs,
        cc: Arc<CompileHandler>,
    ) -> AnySchedulableResponse {
        info!("Preview Arguments: {args:#?}");

        args.preview.control_plane_host = String::default();

        let (resp_tx, resp_rx) = mpsc::unbounded_channel();
        let (ctl_tx, ctl_rx) = mpsc::unbounded_channel();
        let editor_conn = LspEditorConnection { ctl_rx, resp_tx };

        let chandle = cc.clone();
        let previewer = preview(
            args.preview,
            move |handle| {
                chandle.register_preview(handle);
                chandle
            },
            Some(editor_conn),
            TYPST_PREVIEW_HTML,
        );

        let client = self.client.clone();
        self.client.handle.spawn(async move {
            let mut resp_rx = resp_rx;
            while let Some(resp) = resp_rx.recv().await {
                use ControlPlaneResponse::*;

                match resp {
                    // ignoring compile status per task.
                    CompileStatus(..) => {}
                    SyncEditorChanges(..) => {
                        log::warn!("preview is sending SyncEditorChanges in lsp mode");
                    }
                    EditorScrollTo(s) => client.send_notification::<ScrollSource>(s),
                    Outline(s) => client.send_notification::<NotifDocumentOutline>(s),
                }
            }

            info!("PreviewState: Response channel closed");
        });

        let preview_tx = self.preview_tx.clone();
        just_future!(async move {
            let previewer = previewer.await;

            let (static_server_addr, static_server_killer, static_server_handle) =
                make_static_host(&previewer, args.static_file_host, args.preview_mode);
            info!("Static file server listening on: {}", static_server_addr);

            let resp = StartPreviewResponse {
                static_server_port: Some(static_server_addr.port()),
                static_server_addr: Some(static_server_addr.to_string()),
                data_plane_port: Some(previewer.data_plane_port()),
            };

            let sent = preview_tx.send(PreviewRequest::Started(PreviewTab {
                task_id: args.task_id.clone(),
                previewer,
                static_server_killer,
                static_server_handle,
                ctl_tx,
                cc,
            }));
            sent.map_err(|_| internal_error("failed to register preview tab"))?;

            Ok(serde_json::to_value(resp).unwrap())
        })
    }

    pub fn kill(&self, task_id: String) -> AnySchedulableResponse {
        let (tx, rx) = oneshot::channel();

        let sent = self.preview_tx.send(PreviewRequest::Kill(task_id, tx));
        sent.map_err(|_| internal_error("failed to send kill request"))?;

        just_future!(async move { rx.await.map_err(|_| internal_error("cancelled"))? })
    }

    pub fn scroll(&self, task_id: String, req: JsonValue) -> AnySchedulableResponse {
        let req = serde_json::from_value(req).map_err(|e| internal_error(e.to_string()))?;

        let sent = self.preview_tx.send(PreviewRequest::Scroll(task_id, req));
        sent.map_err(|_| internal_error("failed to send scroll request"))?;

        just_ok!(JsonValue::Null)
    }
}

#[path = "preview_compiler.rs"]
mod compiler;
use compiler::CompileServer;

use crate::{compile_init::CompileOnceArgs, LspUniverse};

pub fn make_static_host(
    previewer: &Previewer,
    static_file_addr: String,
    mode: PreviewMode,
) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let frontend_html = previewer.frontend_html(mode);
    let make_service = make_service_fn(move |_| {
        let html = frontend_html.clone();
        async move {
            Ok::<_, hyper::http::Error>(service_fn(move |req| {
                // todo: clone may not be necessary
                let html = html.as_ref().to_owned();
                async move {
                    if req.uri().path() == "/" {
                        log::info!("Serve frontend: {:?}", mode);
                        Ok::<_, Error>(hyper::Response::new(hyper::Body::from(html)))
                    } else if req.uri().path() == "/await_tree" {
                        Ok::<_, Error>(hyper::Response::new(hyper::Body::from(
                            get_await_tree_async().await,
                        )))
                    } else {
                        // jump to /
                        let mut res = hyper::Response::new(hyper::Body::empty());
                        *res.status_mut() = hyper::StatusCode::FOUND;
                        res.headers_mut().insert(
                            hyper::header::LOCATION,
                            hyper::header::HeaderValue::from_static("/"),
                        );
                        Ok(res)
                    }
                }
            }))
        }
    });
    let server = hyper::Server::bind(&static_file_addr.parse().unwrap()).serve(make_service);
    let addr = server.local_addr();

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let graceful = server.with_graceful_shutdown(async {
        rx.await.ok();
    });

    let join_handle = tokio::spawn(async move {
        if let Err(e) = graceful.await {
            error!("Static file server error: {}", e);
        }
    });
    (addr, tx, join_handle)
}

/// Entry point.
pub async fn preview_main(args: PreviewCliArgs) -> anyhow::Result<()> {
    let async_root = REGISTRY
        .lock()
        .await
        .register("root".into(), "typst-preview");
    info!("Arguments: {:#?}", args);
    let input = args.compile.input.context("entry file must be provided")?;
    let input = Path::new(&input);
    let entry = if input.is_absolute() {
        input.to_owned()
    } else {
        std::env::current_dir().unwrap().join(input)
    };
    let inputs = args
        .compile
        .inputs
        .iter()
        .map(|(k, v)| (Str::from(k.as_str()), Value::Str(Str::from(v.as_str()))))
        .collect();
    let root = if let Some(root) = &args.compile.root {
        if root.is_absolute() {
            root.clone()
        } else {
            std::env::current_dir().unwrap().join(root)
        }
    } else {
        std::env::current_dir().unwrap()
    };
    if !entry.starts_with(&root) {
        error!("entry file must be in the root directory");
        std::process::exit(1);
    }

    let world = {
        let world = LspUniverse::new(CompileOpts {
            entry: EntryOpts::new_rooted(root.clone(), Some(entry.clone())),
            inputs,
            no_system_fonts: args.compile.font.ignore_system_fonts,
            font_paths: args.compile.font.font_paths.clone(),
            with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
            ..CompileOpts::default()
        })
        .expect("incorrect options");

        world.with_entry_file(entry)
    };

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        info!("Ctrl-C received, exiting");
        std::process::exit(0);
    });

    let previewer = preview(
        args.preview,
        move |handle| {
            let compile_server = CompileServer::new(world, handle);

            compile_server.spawn().unwrap()
        },
        None,
        TYPST_PREVIEW_HTML,
    );
    let previewer = async_root
        .instrument(previewer)
        .instrument_await("preview")
        .await;

    let static_file_addr = args.static_file_host;
    let mode = args.preview_mode;
    let (static_server_addr, _, static_server_handle) =
        make_static_host(&previewer, static_file_addr, mode);
    info!("Static file server listening on: {}", static_server_addr);
    if !args.dont_open_in_browser {
        if let Err(e) = open::that_detached(format!("http://{}", static_server_addr)) {
            error!("failed to open browser: {}", e);
        };
    }
    let _ = tokio::join!(previewer.join(), static_server_handle);

    Ok(())
}

#[derive(Serialize, Deserialize)]
struct DisposePreview {
    task_id: String,
}

impl Notification for DisposePreview {
    type Params = Self;
    const METHOD: &'static str = "tinymist/preview/dispose";
}

struct ScrollSource;

impl Notification for ScrollSource {
    type Params = DocToSrcJumpInfo;
    const METHOD: &'static str = "tinymist/preview/scrollSource";
}

struct NotifDocumentOutline;

impl Notification for NotifDocumentOutline {
    type Params = typst_preview::Outline;
    const METHOD: &'static str = "tinymist/documentOutline";
}
