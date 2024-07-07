use std::{borrow::Cow, collections::HashMap, net::SocketAddr, path::Path, sync::Arc};

use anyhow::Context;
use await_tree::InstrumentAwait;
use hyper::{
    service::{make_service_fn, service_fn},
    Error,
};
use lsp_types::notification::Notification;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sync_lsp::just_ok;
use tinymist_assets::TYPST_PREVIEW_HTML;
use tinymist_query::{analysis::Analysis, PositionEncoding};
use tokio::sync::{mpsc, oneshot, watch};
use typst::foundations::{Str, Value};
use typst_preview::{
    await_tree::{get_await_tree_async, REGISTRY},
    preview, ControlPlaneMessage, ControlPlaneResponse, DocToSrcJumpInfo, LspControlPlaneRx,
    LspControlPlaneTx, PreviewArgs, PreviewMode, Previewer,
};
use typst_ts_core::config::{compiler::EntryOpts, CompileOpts};

use super::*;
use crate::{compile_init::CompileOnceArgs, LspUniverse};
use actor::{typ_client::CompileHandler, typ_server::CompileServerActor};

#[derive(Debug, Clone, clap::Parser)]
pub struct PreviewCliArgs {
    #[clap(flatten)]
    pub preview: PreviewArgs,

    #[clap(flatten)]
    pub compile: CompileOnceArgs,

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
                    log::info!("PreviewTask({task_id}): killing");
                    let Some(mut tab) = self.tabs.remove(&task_id) else {
                        let _ = tx.send(Err(internal_error("task not found")));
                        continue;
                    };

                    let client = self.client.clone();
                    self.client.handle.spawn(async move {
                        tab.previewer.stop().await;
                        let _ = tab.ss_killer.send(());

                        // Wait for previewer to stop
                        log::info!("PreviewTask({task_id}): wait for previewer to stop");
                        tab.previewer.join().await;
                        log::info!("PreviewTask({task_id}): wait for static server to stop");
                        let _ = tab.ss_handle.await;

                        log::info!("PreviewTask({task_id}): killed");
                        // Unregister preview
                        tab.compile_handler.unregister_preview(&tab.task_id);
                        // Send response
                        let _ = tx.send(Ok(JsonValue::Null));
                        // Send global notification
                        client.send_notification::<DisposePreview>(DisposePreview { task_id });
                    });
                }
                PreviewRequest::Scroll(task_id, req) => {
                    self.scroll(task_id, req).await;
                }
            }
        }
    }

    async fn scroll(&mut self, task_id: String, req: ControlPlaneMessage) -> Option<()> {
        self.tabs.get(&task_id)?.ctl_tx.send(req).ok()
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

pub struct PreviewTab {
    /// Task ID
    pub task_id: String,
    /// Previewer
    pub previewer: Previewer,
    /// Static server killer
    pub ss_killer: oneshot::Sender<()>,
    /// Static server handle
    pub ss_handle: tokio::task::JoinHandle<()>,
    /// Control plane message sender
    pub ctl_tx: mpsc::UnboundedSender<ControlPlaneMessage>,
    /// Compile handler
    pub compile_handler: Arc<CompileHandler>,
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
        compile_handler: Arc<CompileHandler>,
    ) -> AnySchedulableResponse {
        let task_id = args.preview.task_id.clone();
        log::info!("PreviewTask({task_id}): arguments: {args:#?}");

        // Disble control plane host
        args.preview.control_plane_host = String::default();

        let (lsp_tx, lsp_rx) = LspControlPlaneTx::new();
        let LspControlPlaneRx {
            resp_rx,
            ctl_tx,
            mut shutdown_rx,
        } = lsp_rx;

        // Ensure the preview can receive a first compilation.
        let tid = task_id.clone();
        let cc = compile_handler.clone();
        let snap = compile_handler.snapshot();
        let doc_rx = compile_handler.doc_tx.subscribe();
        let compile_fence = async move {
            let now = reflexo::time::now();
            // The fence
            snap.ok()?.snapshot().await.ok()?.compile();

            let w = cc.inner.read();
            let w = w.as_ref()?;
            if w.task_id() != tid {
                return None;
            }

            // But we just send a latest document to the previewer.
            let latest_doc = doc_rx.borrow().clone();
            let has_doc = latest_doc.is_some();
            let elapsed = now.elapsed().unwrap_or_default();
            log::info!("PreviewTask({tid}): put fence in {elapsed:?}? {has_doc}");
            w.notify_compile(Ok(latest_doc?), true);

            Some(())
        };

        // Create a previewer
        let previewer = preview(
            args.preview,
            compile_handler.clone(),
            Some(lsp_tx),
            TYPST_PREVIEW_HTML,
        );

        // Forward preview responses to lsp client
        let tid = task_id.clone();
        let client = self.client.clone();
        self.client.handle.spawn(async move {
            let mut resp_rx = resp_rx;
            while let Some(resp) = resp_rx.recv().await {
                use ControlPlaneResponse::*;

                match resp {
                    // ignoring compile status per task.
                    CompileStatus(..) => {}
                    SyncEditorChanges(..) => {
                        log::warn!("PreviewTask({tid}): is sending SyncEditorChanges in lsp mode");
                    }
                    EditorScrollTo(s) => client.send_notification::<ScrollSource>(s),
                    Outline(s) => client.send_notification::<NotifDocumentOutline>(s),
                }
            }

            log::info!("PreviewTask({tid}): response channel closed");
        });

        // Process preview shutdown
        let tid = task_id.clone();
        let preview_tx = self.preview_tx.clone();
        self.client.handle.spawn(async move {
            // shutdown_rx
            let Some(()) = shutdown_rx.recv().await else {
                return;
            };

            log::info!("PreviewTask({tid}): internal killing");
            let (tx, rx) = oneshot::channel();
            preview_tx.send(PreviewRequest::Kill(tid.clone(), tx)).ok();
            rx.await.ok();
            log::info!("PreviewTask({tid}): internal killed");
        });

        let preview_tx = self.preview_tx.clone();
        let handle = self.client.handle.clone();
        just_future!(async move {
            let previewer = previewer.await;
            compile_handler.register_preview(previewer.compile_watcher().clone());

            // Put a fence to ensure the previewer can receive the first compilation.   z
            // The fence must be put after the previewer is initialized.
            handle.spawn(compile_fence);

            let (ss_addr, ss_killer, ss_handle) =
                make_static_host(&previewer, args.static_file_host, args.preview_mode);
            log::info!("PreviewTask({task_id}): static file server listening on: {ss_addr}");

            let resp = StartPreviewResponse {
                static_server_port: Some(ss_addr.port()),
                static_server_addr: Some(ss_addr.to_string()),
                data_plane_port: Some(previewer.data_plane_port()),
            };

            let sent = preview_tx.send(PreviewRequest::Started(PreviewTab {
                task_id,
                previewer,
                ss_killer,
                ss_handle,
                ctl_tx,
                compile_handler,
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

    pub fn scroll(&self, task_id: String, req: ControlPlaneMessage) -> AnySchedulableResponse {
        let sent = self.preview_tx.send(PreviewRequest::Scroll(task_id, req));
        sent.map_err(|_| internal_error("failed to send scroll request"))?;

        just_ok!(JsonValue::Null)
    }
}

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

    let (tx, rx) = tokio::sync::oneshot::channel();
    let (final_tx, final_rx) = tokio::sync::oneshot::channel();
    let graceful = server.with_graceful_shutdown(async {
        final_rx.await.ok();
        log::info!("Static file server stop requested");
    });

    let join_handle = tokio::spawn(async move {
        tokio::select! {
            Err(err) = graceful => {
                log::error!("Static file server error: {err:?}");
            }
            _ = rx => {
                final_tx.send(()).ok();
            }
        }
        log::info!("Static file server joined");
    });
    (addr, tx, join_handle)
}

/// Entry point.
pub async fn preview_main(args: PreviewCliArgs) -> anyhow::Result<()> {
    let async_root = REGISTRY
        .lock()
        .await
        .register("root".into(), "typst-preview");
    log::info!("Arguments: {args:#?}");

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        log::info!("Ctrl-C received, exiting");
        std::process::exit(0);
    });

    let entry = {
        let input = args.compile.input.context("entry file must be provided")?;
        let input = Path::new(&input);
        let entry = if input.is_absolute() {
            input.to_owned()
        } else {
            std::env::current_dir().unwrap().join(input)
        };

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
            log::error!("entry file must be in the root directory");
            std::process::exit(1);
        }

        let relative_entry = match entry.strip_prefix(&root) {
            Ok(e) => e,
            Err(_) => {
                log::error!("entry path must be inside the root: {}", entry.display());
                std::process::exit(1);
            }
        };

        EntryOpts::new_rooted(root.clone(), Some(relative_entry.to_owned()))
    };

    let inputs = args
        .compile
        .inputs
        .iter()
        .map(|(k, v)| (Str::from(k.as_str()), Value::Str(Str::from(v.as_str()))))
        .collect();

    let world = LspUniverse::new(CompileOpts {
        entry,
        inputs,
        no_system_fonts: args.compile.font.ignore_system_fonts,
        font_paths: args.compile.font.font_paths.clone(),
        with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
        ..CompileOpts::default()
    })
    .expect("incorrect options");

    let (service, handle) = {
        // type EditorSender = mpsc::UnboundedSender<EditorRequest>;
        let (doc_tx, _) = watch::channel(None);
        let (export_tx, mut export_rx) = mpsc::unbounded_channel();
        let (editor_tx, mut editor_rx) = mpsc::unbounded_channel();
        let (intr_tx, intr_rx) = mpsc::unbounded_channel();

        let handle = Arc::new(CompileHandler {
            inner: Default::default(),
            diag_group: "main".to_owned(),
            intr_tx: intr_tx.clone(),
            doc_tx,
            export_tx,
            editor_tx,
            analysis: Analysis {
                position_encoding: PositionEncoding::Utf16,
                enable_periscope: false,
                caches: Default::default(),
            },
            periscope: tinymist_render::PeriscopeRenderer::default(),
        });

        // Consume export_tx and editor_rx
        tokio::spawn(async move { while export_rx.recv().await.is_some() {} });
        tokio::spawn(async move { while editor_rx.recv().await.is_some() {} });

        let service =
            CompileServerActor::new(world, intr_tx, intr_rx).with_watch(Some(handle.clone()));

        (service, handle)
    };

    let previewer = preview(args.preview, handle.clone(), None, TYPST_PREVIEW_HTML);

    let previewer = async_root
        .instrument(previewer)
        .instrument_await("preview")
        .await;

    handle.register_preview(previewer.compile_watcher().clone());
    tokio::spawn(service.spawn().instrument_await("spawn typst server"));

    let (static_server_addr, _tx, static_server_handle) =
        make_static_host(&previewer, args.static_file_host, args.preview_mode);
    log::info!("Static file server listening on: {}", static_server_addr);

    if !args.dont_open_in_browser {
        if let Err(e) = open::that_detached(format!("http://{}", static_server_addr)) {
            log::error!("failed to open browser: {}", e);
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
