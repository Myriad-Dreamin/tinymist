//! Document preview tool for Typst

pub use compile::{PreviewCompileView, ProjectPreviewHandler};
pub use http::{make_http_server, HttpServer};

mod compile;
mod http;

use std::{collections::HashMap, path::Path, sync::Arc};

use clap::Parser;
use futures::{SinkExt, StreamExt, TryStreamExt};
use hyper_tungstenite::{tungstenite::Message, HyperWebsocket, HyperWebsocketStream};
use lsp_types::notification::Notification;
use lsp_types::Url;
use reflexo_typst::error::prelude::*;
use serde::Serialize;
use serde_json::Value as JsonValue;
use sync_ls::just_ok;
use tinymist_assets::TYPST_PREVIEW_HTML;
use tinymist_query::{LspPosition, LspRange};
use tinymist_std::error::IgnoreLogging;
use tokio::sync::{mpsc, oneshot};
use typst_preview::{
    frontend_html, ControlPlaneMessage, ControlPlaneRx, ControlPlaneTx, DocToSrcJumpInfo,
    PreviewArgs, PreviewBuilder, PreviewMode, Previewer, WsMessage,
};

use crate::actor::preview::{PreviewActor, PreviewRequest, PreviewTab};
use crate::project::{ProjectInsId, ProjectPreviewState, WorldProvider};
use crate::tool::project::{start_project, ProjectOpts, StartProjectResult};
use crate::*;

/// The kind of the preview.
pub enum PreviewKind {
    /// Previews a specific file.
    Regular,
    /// Walks through the project and previews the main file related to the
    /// current focused file.
    Browsing,
    /// Runs a browsing preview in background.
    Background,
}

/// CLI Arguments for the preview tool.
#[derive(Debug, Clone, clap::Parser)]
pub struct PreviewCliArgs {
    /// Preview arguments
    #[clap(flatten)]
    pub preview: PreviewArgs,

    /// Compile arguments
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// Preview mode
    #[clap(long = "preview-mode", default_value = "document", value_name = "MODE")]
    pub preview_mode: PreviewMode,

    /// Data plane server will bind to this address. Note: if it equals to
    /// `static_file_host`, same address will be used.
    #[clap(
        long = "data-plane-host",
        default_value = "127.0.0.1:23625",
        value_name = "HOST",
        hide(true)
    )]
    pub data_plane_host: String,

    /// Control plane server will bind to this address
    #[clap(
        long = "control-plane-host",
        default_value = "127.0.0.1:23626",
        value_name = "HOST",
        hide(true)
    )]
    pub control_plane_host: String,

    /// (Deprecated) (File) Host for the preview server. Note: if it equals to
    /// `data_plane_host`, same address will be used.
    #[clap(
        long = "host",
        value_name = "HOST",
        default_value = "",
        alias = "static-file-host"
    )]
    pub static_file_host: String,

    /// Let it not be the primary instance.
    #[clap(long = "not-primary", hide(true))]
    pub not_as_primary: bool,

    /// Open the preview in the browser after compilation. If `--no-open` is
    /// set, this flag will be ignored.
    #[clap(long = "open")]
    pub open: bool,

    /// Don't open the preview in the browser after compilation. If `--open` is
    /// set as well, this flag will win.
    #[clap(long = "no-open")]
    pub no_open: bool,
}

impl PreviewCliArgs {
    /// Whether to open the preview in the browser after compilation.
    pub fn open_in_browser(&self, default: bool) -> bool {
        !self.no_open && (self.open || default)
    }
}

/// Response for starting a preview.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPreviewResponse {
    static_server_port: Option<u16>,
    static_server_addr: Option<String>,
    data_plane_port: Option<u16>,
    is_primary: bool,
}

impl ServerState {
    /// Starts a background preview instance.
    pub fn background_preview(&mut self) {
        if !self.config.preview.background.enabled {
            return;
        }

        let args = self.config.preview.background.args.clone();
        let args = args.unwrap_or_else(|| {
            vec![
                "--data-plane-host=127.0.0.1:23635".to_string(),
                "--invert-colors=auto".to_string(),
            ]
        });

        let res = self.start_preview(args, PreviewKind::Background);

        // todo: looks ugly
        self.client.handle.spawn(async move {
            let fut = match res {
                Ok(fut) => fut,
                Err(e) => {
                    log::error!("failed to start background preview: {e:?}");
                    return;
                }
            };
            tokio::pin!(fut);
            let () = fut.as_mut().await;

            if let Some(Err(e)) = fut.as_mut().take_output() {
                log::error!("failed to start background preview: {e:?}");
            }
        });
    }

    /// Starts a preview instance.
    pub fn start_preview(
        &mut self,
        cli_args: Vec<String>,
        kind: PreviewKind,
    ) -> SchedulableResponse<StartPreviewResponse> {
        // clap parse
        let cli_args = ["preview"]
            .into_iter()
            .chain(cli_args.iter().map(|e| e.as_str()));
        let cli_args =
            PreviewCliArgs::try_parse_from(cli_args).map_err(|e| invalid_params(e.to_string()))?;

        // todo: preview specific arguments are not used
        let entry = cli_args.compile.input.as_ref();
        let entry = entry
            .map(|input| {
                let input = Path::new(&input);
                if !input.is_absolute() {
                    // std::env::current_dir().unwrap().join(input)
                    return Err(invalid_params("entry file must be absolute path"));
                };

                Ok(input.into())
            })
            .transpose()?;

        let task_id = cli_args.preview.task_id.clone();
        if task_id == "primary" {
            return Err(invalid_params("task id 'primary' is reserved"));
        }

        if cli_args.not_as_primary && matches!(kind, PreviewKind::Background) {
            return Err(invalid_params(
                "cannot start background preview as non-primary",
            ));
        }

        let previewer = typst_preview::PreviewBuilder::new(cli_args.preview.clone());
        let watcher = previewer.compile_watcher();

        let primary = &mut self.project.compiler.primary;
        // todo: recover pin status reliably
        let is_browsing = matches!(kind, PreviewKind::Browsing | PreviewKind::Background);
        let is_background = matches!(kind, PreviewKind::Background);

        let registered_as_primary = !cli_args.not_as_primary
            && (is_browsing || entry.is_some())
            && self.preview.watchers.register(&primary.id, watcher);
        if matches!(kind, PreviewKind::Background) && !registered_as_primary {
            return Err(invalid_params(
                "failed to register background preview to the primary instance",
            ));
        }

        if registered_as_primary {
            let id = primary.id.clone();

            if let Some(entry) = entry {
                self.change_main_file(Some(entry)).map_err(internal_error)?;
            }
            self.set_pin_by_preview(true, is_browsing);

            self.preview
                .start(cli_args, previewer, id, true, is_background)
        } else if let Some(entry) = entry {
            let id = self
                .restart_dedicate(&task_id, Some(entry))
                .map_err(internal_error)?;

            if !self.project.preview.register(&id, watcher) {
                return Err(invalid_params(
                    "cannot register preview to the compiler instance",
                ));
            }

            self.preview
                .start(cli_args, previewer, id, false, is_background)
        } else {
            return Err(internal_error("entry file must be provided"));
        }
    }
}

/// The global state of the preview tool.
pub struct PreviewState {
    /// Connection to the LSP client.
    client: TypedLspClient<PreviewState>,
    /// The backend running actor.
    preview_tx: mpsc::UnboundedSender<PreviewRequest>,
    /// the watchers for the preview
    pub(crate) watchers: ProjectPreviewState,
    /// Whether to send show document requests with customized notification.
    pub customized_show_document: bool,
}

impl PreviewState {
    /// Create a new preview state.
    pub fn new(
        config: &Config,
        watchers: ProjectPreviewState,
        client: TypedLspClient<PreviewState>,
    ) -> Self {
        let (preview_tx, preview_rx) = mpsc::unbounded_channel();

        client.handle.spawn(
            PreviewActor {
                client: client.clone().to_untyped(),
                tabs: HashMap::default(),
                preview_rx,
                watchers: watchers.clone(),
            }
            .run(),
        );

        Self {
            client,
            preview_tx,
            watchers,
            customized_show_document: config.customized_show_document,
        }
    }

    pub(crate) fn stop_all(&mut self) {
        log::info!("Stopping all preview tasks");

        let mut watchers = self.watchers.inner.lock();
        for (_, watcher) in watchers.iter_mut() {
            self.preview_tx
                .send(PreviewRequest::Kill(
                    watcher.task_id().to_owned(),
                    oneshot::channel().0,
                ))
                .log_error_with(|| format!("failed to send kill request({:?})", watcher.task_id()));
        }
        watchers.clear();
    }
}

impl PreviewState {
    /// Start a preview on a given compiler.
    pub fn start(
        &self,
        args: PreviewCliArgs,
        previewer: PreviewBuilder,
        // compile_handler: Arc<CompileHandler>,
        project_id: ProjectInsId,
        is_primary: bool,
        is_background: bool,
    ) -> SchedulableResponse<StartPreviewResponse> {
        let compile_handler = Arc::new(ProjectPreviewHandler {
            project_id,
            client: Box::new(self.client.clone().to_untyped()),
        });

        let task_id = args.preview.task_id.clone();
        let open_in_browser = args.open_in_browser(false);
        log::info!("PreviewTask({task_id}): arguments: {args:#?}");

        if !args.static_file_host.is_empty() && (args.static_file_host != args.data_plane_host) {
            return Err(internal_error("--static-file-host is removed"));
        }

        let (lsp_tx, lsp_rx) = ControlPlaneTx::new(false);
        let ControlPlaneRx {
            resp_rx,
            ctl_tx,
            mut shutdown_rx,
        } = lsp_rx;

        let (websocket_tx, websocket_rx) = mpsc::unbounded_channel();

        let previewer = previewer.build(lsp_tx, compile_handler.clone());

        // Forward preview responses to lsp client
        let tid = task_id.clone();
        let client = self.client.clone();
        let customized_show_document = self.customized_show_document;
        self.client.handle.spawn(async move {
            let mut resp_rx = resp_rx;
            while let Some(resp) = resp_rx.recv().await {
                use typst_preview::ControlPlaneResponse::*;

                match resp {
                    // ignoring compile status per task.
                    CompileStatus(..) => {}
                    SyncEditorChanges(..) => {
                        log::warn!("PreviewTask({tid}): is sending SyncEditorChanges in lsp mode");
                    }
                    EditorScrollTo(s) => {
                        if customized_show_document {
                            client.send_notification::<ScrollSource>(&s)
                        } else {
                            send_show_document(&client, &s, &tid);
                        }
                    }
                    Outline(s) => client.send_notification::<NotifDocumentOutline>(&s),
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
        just_future(async move {
            let mut previewer = previewer.await;
            bind_streams(&mut previewer, websocket_rx);

            // Put a fence to ensure the previewer can receive the first compilation.
            // The fence must be put after the previewer is initialized.
            compile_handler.flush_compile();

            // Replace the data plane port in the html to self
            let frontend_html = frontend_html(TYPST_PREVIEW_HTML, args.preview_mode, "/");

            let srv = make_http_server(frontend_html, args.data_plane_host, websocket_tx).await;
            let addr = srv.addr;
            log::info!("PreviewTask({task_id}): preview server listening on: {addr}");

            let resp = StartPreviewResponse {
                static_server_port: Some(addr.port()),
                static_server_addr: Some(addr.to_string()),
                data_plane_port: Some(addr.port()),
                is_primary,
            };

            if open_in_browser {
                open::that_detached(format!("http://127.0.0.1:{}", addr.port()))
                    .log_error("failed to open browser for preview");
            }

            let sent = preview_tx.send(PreviewRequest::Started(PreviewTab {
                task_id,
                previewer,
                srv,
                ctl_tx,
                compile_handler,
                is_primary,
                is_background,
            }));
            sent.map_err(|_| internal_error("failed to register preview tab"))?;

            Ok(resp)
        })
    }

    /// Kill a preview task. Ignore if the task is not found.
    pub fn kill(&self, task_id: String) -> AnySchedulableResponse {
        let (tx, rx) = oneshot::channel();

        let sent = self.preview_tx.send(PreviewRequest::Kill(task_id, tx));
        sent.map_err(|_| internal_error("failed to send kill request"))?;

        just_future(async move { rx.await.map_err(|_| internal_error("cancelled"))? })
    }

    /// Scroll the preview to a given position.
    pub fn scroll(&self, task_id: String, req: ControlPlaneMessage) -> AnySchedulableResponse {
        let sent = self.preview_tx.send(PreviewRequest::Scroll(task_id, req));
        sent.map_err(|_| internal_error("failed to send scroll request"))?;

        just_ok(JsonValue::Null)
    }
}

/// Entry point of the preview tool.
pub async fn preview_main(args: PreviewCliArgs) -> Result<()> {
    log::info!("Arguments: {args:#?}");
    let handle = tokio::runtime::Handle::current();

    let open_in_browser = args.open_in_browser(true);
    let static_file_host =
        if args.static_file_host == args.data_plane_host || !args.static_file_host.is_empty() {
            Some(args.static_file_host)
        } else {
            None
        };

    exit_on_ctrl_c();

    let verse = args.compile.resolve()?;
    let previewer = PreviewBuilder::new(args.preview);

    let (service, handle) = {
        let preview_state = ProjectPreviewState::default();
        let opts = ProjectOpts {
            handle: Some(handle),
            preview: preview_state.clone(),
            ..ProjectOpts::default()
        };

        let StartProjectResult {
            service,
            intr_tx,
            mut editor_rx,
        } = start_project(verse, Some(opts), |compiler, intr, next| {
            next(compiler, intr)
        });

        // Consume editor_rx
        tokio::spawn(async move { while editor_rx.recv().await.is_some() {} });

        let id = service.compiler.primary.id.clone();
        let registered = preview_state.register(&id, previewer.compile_watcher());
        if !registered {
            tinymist_std::bail!("failed to register preview");
        }

        let handle: Arc<ProjectPreviewHandler> = Arc::new(ProjectPreviewHandler {
            project_id: id,
            client: Box::new(intr_tx),
        });

        (service, handle)
    };

    let (lsp_tx, mut lsp_rx) = ControlPlaneTx::new(true);

    let control_plane_server_handle = tokio::spawn(async move {
        let (control_sock_tx, mut control_sock_rx) = mpsc::unbounded_channel();

        let srv =
            make_http_server(String::default(), args.control_plane_host, control_sock_tx).await;
        log::info!("Control panel server listening on: {}", srv.addr);

        let control_websocket = control_sock_rx.recv().await.unwrap();
        let ws = control_websocket.await.unwrap();

        tokio::pin!(ws);

        loop {
            tokio::select! {
                Some(resp) = lsp_rx.resp_rx.recv() => {
                    let r = ws
                        .send(Message::Text(serde_json::to_string(&resp).unwrap()))
                        .await;
                    let Err(err) = r else {
                        continue;
                    };

                    log::warn!("failed to send response to editor {err:?}");
                    break;

                }
                msg = ws.next() => {
                    let msg = match msg {
                        Some(Ok(Message::Text(msg))) => Some(msg),
                        Some(Ok(msg)) => {
                            log::error!("unsupported message: {msg:?}");
                            break;
                        }
                        Some(Err(e)) => {
                            log::error!("failed to receive message: {e}");
                            break;
                        }
                        _ => None,
                    };

                    if let Some(msg) = msg {
                        let Ok(msg) = serde_json::from_str::<ControlPlaneMessage>(&msg) else {
                            log::warn!("failed to parse control plane request: {msg:?}");
                            break;
                        };

                        lsp_rx.ctl_tx.send(msg).unwrap();
                    } else {
                        // todo: inform the editor that the connection is closed.
                        break;
                    }
                }

            }
        }

        let _ = srv.shutdown_tx.send(());
        let _ = srv.join.await;
    });

    let (websocket_tx, websocket_rx) = mpsc::unbounded_channel();
    let mut previewer = previewer.build(lsp_tx, handle.clone()).await;
    tokio::spawn(service.run());

    bind_streams(&mut previewer, websocket_rx);

    let frontend_html = frontend_html(TYPST_PREVIEW_HTML, args.preview_mode, "/");

    let static_server = if let Some(static_file_host) = static_file_host {
        log::warn!("--static-file-host is deprecated, which will be removed in the future. Use --data-plane-host instead.");
        let html = frontend_html.clone();
        Some(make_http_server(html, static_file_host, websocket_tx.clone()).await)
    } else {
        None
    };

    let srv = make_http_server(frontend_html, args.data_plane_host, websocket_tx).await;
    log::info!("Data plane server listening on: {}", srv.addr);

    let static_server_addr = static_server.as_ref().map(|s| s.addr).unwrap_or(srv.addr);
    log::info!("Static file server listening on: {static_server_addr}");

    if open_in_browser {
        open::that_detached(format!("http://{static_server_addr}"))
            .log_error("failed to open browser for preview");
    }

    let _ = tokio::join!(previewer.join(), srv.join, control_plane_server_handle);
    // Assert that the static server's lifetime is longer than the previewer.
    let _s = static_server;

    Ok(())
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

fn send_show_document(client: &TypedLspClient<PreviewState>, s: &DocToSrcJumpInfo, tid: &str) {
    let range_start = s.start.map(|(l, c)| LspPosition {
        line: l as u32,
        character: c as u32,
    });
    let range_end = s.end.map(|(l, c)| LspPosition {
        line: l as u32,
        character: c as u32,
    });
    let range = match (range_start, range_end) {
        (Some(start), Some(end)) => Some(LspRange { start, end }),
        (Some(start), None) | (None, Some(start)) => Some(LspRange { start, end: start }),
        _ => None,
    };

    // todo: resolve uri if any
    let uri = match Url::from_file_path(Path::new(&s.filepath)) {
        Ok(uri) => uri,
        Err(e) => {
            log::error!(
                "PreviewTask({tid}): failed to convert path to URI: {e:?}, path {:?}",
                s.filepath
            );
            return;
        }
    };

    client.send_lsp_request::<lsp_types::request::ShowDocument>(
        lsp_types::ShowDocumentParams {
            uri,
            external: None,
            take_focus: Some(true),
            selection: range,
        },
        |_, resp| {
            if let Some(err) = resp.error {
                log::error!("failed to send ShowDocument request: {err:?}");
            }
        },
    );
}

fn bind_streams(previewer: &mut Previewer, websocket_rx: mpsc::UnboundedReceiver<HyperWebsocket>) {
    previewer.start_data_plane(
        websocket_rx,
        |conn: Result<HyperWebsocketStream, hyper_tungstenite::tungstenite::Error>| {
            let conn = conn.map_err(error_once_map_string!("cannot receive websocket"))?;

            Ok(conn
                .sink_map_err(|e| error_once!("cannot serve_with websocket", err: e.to_string()))
                .map_err(|e| error_once!("cannot serve_with websocket", err: e.to_string()))
                .with(|msg| {
                    Box::pin(async move {
                        let msg = match msg {
                            WsMessage::Text(msg) => Message::Text(msg),
                            WsMessage::Binary(msg) => Message::Binary(msg),
                        };
                        Ok(msg)
                    })
                })
                .map_ok(|msg| match msg {
                    Message::Text(msg) => WsMessage::Text(msg),
                    Message::Binary(msg) => WsMessage::Binary(msg),
                    _ => WsMessage::Text("unsupported message".to_owned()),
                }))
        },
    );
}
