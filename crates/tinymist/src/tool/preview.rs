//! Document preview tool for Typst

pub use compile::{PreviewCompileView, ProjectPreviewHandler};
pub use http::{make_http_server, HttpServer};

mod compile;
mod http;

use std::{collections::HashMap, path::Path, sync::Arc};

use clap::{Parser, ValueEnum};
use futures::{SinkExt, TryStreamExt};
use hyper_tungstenite::{tungstenite::Message, HyperWebsocket, HyperWebsocketStream};
use lsp_types::notification::Notification;
use lsp_types::Url;
use reflexo_typst::error::prelude::*;
use serde::Serialize;
use serde_json::Value as JsonValue;
use sync_ls::just_ok;
use tinymist_assets::TYPST_PREVIEW_HTML;
use tinymist_preview::{
    frontend_html, ControlPlaneMessage, ControlPlaneRx, ControlPlaneTx, DocToSrcJumpInfo,
    PreviewBuilder, PreviewConfig, PreviewMode, Previewer, WsMessage,
};
use tinymist_query::{LspPosition, LspRange};
use tinymist_std::error::IgnoreLogging;
use tokio::sync::{mpsc, oneshot};

use crate::actor::preview::{PreviewActor, PreviewRequest, PreviewTab};
use crate::project::{ProjectInsId, ProjectPreviewState};
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

/// The refresh style for the preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum RefreshStyle {
    /// Refresh preview on save
    #[cfg_attr(feature = "clap", clap(name = "onSave"))]
    OnSave,

    /// Refresh preview on type
    #[cfg_attr(feature = "clap", clap(name = "onType"))]
    #[default]
    OnType,
}

impl From<RefreshStyle> for TaskWhen {
    fn from(style: RefreshStyle) -> Self {
        match style {
            RefreshStyle::OnSave => TaskWhen::OnSave,
            RefreshStyle::OnType => TaskWhen::OnType,
        }
    }
}

/// Specify arguments related to the preview service.
#[derive(Debug, Clone, clap::Parser)]
pub struct PreviewArgs {
    /// Configure the preview mode.
    #[clap(long = "preview-mode", default_value = "document", value_name = "MODE")]
    pub preview_mode: PreviewMode,

    /// Only render visible part of the document.
    ///
    /// This can improve performance but still being experimental.
    #[clap(long = "partial-rendering")]
    pub enable_partial_rendering: Option<bool>,

    /// Configure the way to invert colors of the preview.
    ///
    /// This is useful for dark themes without cost.
    ///
    /// Please note you could see the original colors when you hover elements in
    /// the preview.
    ///
    /// It is also possible to specify strategy to each element kind by an
    /// object map in JSON format.
    ///
    /// Possible element kinds:
    /// - `image`: Images in the preview.
    /// - `rest`: Rest elements in the preview.
    ///
    /// By default, the preview will never invert colors.
    ///
    /// ## Example
    ///
    /// By string:
    ///
    /// ```shell
    /// --invert-colors=auto
    /// ```
    ///
    /// By element:
    ///
    /// ```shell
    /// --invert-colors='{"rest": "always", "image": "never"}'
    /// ```
    #[clap(long)]
    pub invert_colors: Option<String>,

    /// Used by lsp for controlling the preview refresh style.
    ///
    /// This is hidden from the CLI.
    #[clap(long, hide(true))]
    pub refresh_style: Option<RefreshStyle>,
}

impl PreviewArgs {
    /// Get the configuration for the preview.
    pub fn config(&self, config: &PreviewConfig) -> PreviewConfig {
        PreviewConfig {
            enable_partial_rendering: self
                .enable_partial_rendering
                .unwrap_or(config.enable_partial_rendering),
            refresh_style: self
                .refresh_style
                .map(From::from)
                .unwrap_or_else(|| config.refresh_style.clone()),
            invert_colors: match &self.invert_colors {
                Some(s) => s.clone(),
                None => config.invert_colors.clone(),
            },
        }
    }
}

/// Specify arguments related to the preview CLI.
#[derive(Debug, Clone, clap::Parser)]
pub struct PreviewCliArgs {
    /// Configure the preview service.
    #[clap(flatten)]
    pub preview: PreviewArgs,

    /// Specify common arguments to create a world (environment) to run typst
    /// tasks.
    #[clap(flatten)]
    pub compile: CompileOnceArgs,

    /// Used by lsp for identifying the task.
    ///
    /// This is hidden from the CLI.
    #[clap(
        long = "task-id",
        default_value = "default_preview",
        value_name = "TASK_ID",
        hide(true)
    )]
    pub task_id: String,

    /// Configure the data plane server address.
    ///
    /// Note: if it equals to `static_file_host`, same address will be used.
    #[clap(
        long = "data-plane-host",
        default_value = "127.0.0.1:23625",
        value_name = "HOST",
        hide(true)
    )]
    pub data_plane_host: String,

    /// Configure the control plane server address.
    #[clap(
        long = "control-plane-host",
        default_value = "127.0.0.1:23626",
        value_name = "HOST",
        hide(true)
    )]
    pub control_plane_host: String,

    /// (Deprecated) Configure (File) Host address for the preview server.
    ///
    /// Note: if it equals to `data_plane_host`, same address will be used.
    #[clap(
        long = "host",
        value_name = "HOST",
        default_value = "",
        alias = "static-file-host"
    )]
    pub static_file_host: String,

    /// Let it not be the primary instance.
    ///
    /// This is hidden from the CLI.
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
    /// Determines whether to open the preview in the browser after compilation.
    pub fn open_in_browser(&self, default: bool) -> bool {
        !self.no_open && (self.open || default)
    }
}

/// Response for starting a preview instance.
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
        // default configs
        let config = cli_args.preview.config(&self.config.preview());

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

        let task_id = cli_args.task_id.clone();
        if task_id == "primary" {
            return Err(invalid_params("task id 'primary' is reserved"));
        }

        if cli_args.not_as_primary && matches!(kind, PreviewKind::Background) {
            return Err(invalid_params(
                "cannot start background preview as non-primary",
            ));
        }

        let previewer = tinymist_preview::PreviewBuilder::new(config);
        let watcher = previewer.compile_watcher(task_id.clone());

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
            Err(internal_error("entry file must be provided"))
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

        let task_id = args.task_id.clone();
        #[cfg(feature = "open")]
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
                use tinymist_preview::ControlPlaneResponse::*;

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
            let frontend_html = frontend_html(TYPST_PREVIEW_HTML, args.preview.preview_mode, "/");

            let srv = make_http_server(frontend_html, args.data_plane_host, websocket_tx).await;
            let addr = srv.addr;
            log::info!("PreviewTask({task_id}): preview server listening on: {addr}");

            let resp = StartPreviewResponse {
                static_server_port: Some(addr.port()),
                static_server_addr: Some(addr.to_string()),
                data_plane_port: Some(addr.port()),
                is_primary,
            };

            #[cfg(feature = "open")]
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

    /// Kill all preview tasks.
    pub fn kill_all(&self) -> AnySchedulableResponse {
        let (tx, rx) = oneshot::channel();

        let sent = self.preview_tx.send(PreviewRequest::KillAll(tx));
        sent.map_err(|_| internal_error("failed to send kill request"))?;

        just_future(async move { rx.await.map_err(|_| internal_error("cancelled"))? })
    }

    /// Scroll the preview to a given position.
    pub fn scroll(&self, task_id: String, req: ControlPlaneMessage) -> AnySchedulableResponse {
        let sent = self.preview_tx.send(PreviewRequest::Scroll(task_id, req));
        sent.map_err(|_| internal_error("failed to send scroll request"))?;

        just_ok(JsonValue::Null)
    }

    /// Scroll all preview panels to a given position.
    pub fn scroll_all(&self, req: ControlPlaneMessage) -> AnySchedulableResponse {
        let sent = self.preview_tx.send(PreviewRequest::ScrollAll(req));
        sent.map_err(|_| internal_error("failed to send scroll request"))?;

        just_ok(JsonValue::Null)
    }
}

struct ScrollSource;

impl Notification for ScrollSource {
    type Params = DocToSrcJumpInfo;
    const METHOD: &'static str = "tinymist/preview/scrollSource";
}

struct NotifDocumentOutline;

impl Notification for NotifDocumentOutline {
    type Params = tinymist_preview::Outline;
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

/// Bind the hyper websocket streams to the previewer.
pub fn bind_streams(
    previewer: &mut Previewer,
    websocket_rx: mpsc::UnboundedReceiver<HyperWebsocket>,
) {
    previewer.start_data_plane(
        websocket_rx,
        |conn: Result<HyperWebsocketStream, hyper_tungstenite::tungstenite::Error>| {
            let conn: hyper_tungstenite::WebSocketStream<
                hyper_util::rt::TokioIo<hyper::upgrade::Upgraded>,
            > = conn.map_err(error_once_map_string!("cannot receive websocket"))?;

            Ok(conn
                .sink_map_err(|e| error_once!("cannot serve_with websocket", err: e.to_string()))
                .map_err(|e| error_once!("cannot serve_with websocket", err: e.to_string()))
                .with(|msg| {
                    Box::pin(async move {
                        Ok(match msg {
                            WsMessage::Text(msg) => Message::text(msg),
                            WsMessage::Binary(msg) => Message::Binary(msg),
                            WsMessage::Ping(msg) => Message::Ping(msg),
                            WsMessage::Pong(msg) => Message::Pong(msg),
                        })
                    })
                })
                .map_ok(|msg| match msg {
                    Message::Text(msg) => WsMessage::Text(msg.as_str().to_owned()),
                    Message::Binary(msg) => WsMessage::Binary(msg),
                    Message::Ping(msg) => WsMessage::Ping(msg),
                    Message::Pong(msg) => WsMessage::Pong(msg),
                    Message::Close(..) => WsMessage::Text("bad_client_msg: Close".to_owned()),
                    Message::Frame(..) => WsMessage::Text("bad_client_msg: Frame".to_owned()),
                }))
        },
    );
}
