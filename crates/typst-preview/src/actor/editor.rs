use std::sync::Arc;

use reflexo_typst::debug_loc::DocumentPosition;
use serde::{Deserialize, Serialize};
use tinymist_std::error::IgnoreLogging;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

use crate::actor::render::RenderActorRequest;
use crate::debug_loc::{InternQuery, SpanInterner};
use crate::outline::Outline;
use crate::{
    ChangeCursorPositionRequest, DocToSrcJumpInfo, EditorServer, MemoryFiles, MemoryFilesShort,
    ResolveSourceLocRequest,
};

use super::webview::WebviewActorRequest;
#[derive(Debug, Clone, Deserialize)]
pub struct DocToSrcJumpResolveRequest {
    /// Span id in hex-format.
    pub span: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PanelScrollByPositionRequest {
    position: DocumentPosition,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum CompileStatus {
    Compiling,
    CompileSuccess,
    CompileError,
}

#[derive(Debug)]
pub enum EditorActorRequest {
    Shutdown,
    DocToSrcJumpResolve(DocToSrcJumpResolveRequest),
    DocToSrcJump(DocToSrcJumpInfo),
    Outline(Outline),
    CompileStatus(CompileStatus),
}

pub struct ControlPlaneTx {
    pub is_standalone: bool,
    pub resp_tx: mpsc::UnboundedSender<ControlPlaneResponse>,
    pub ctl_rx: mpsc::UnboundedReceiver<ControlPlaneMessage>,
    pub shutdown_tx: mpsc::Sender<()>,
}

pub struct ControlPlaneRx {
    pub resp_rx: mpsc::UnboundedReceiver<ControlPlaneResponse>,
    pub ctl_tx: mpsc::UnboundedSender<ControlPlaneMessage>,
    pub shutdown_rx: mpsc::Receiver<()>,
}

impl ControlPlaneTx {
    pub fn new(need_sync_files: bool) -> (ControlPlaneTx, ControlPlaneRx) {
        let (resp_tx, resp_rx) = mpsc::unbounded_channel();
        let (ctl_tx, ctl_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        (
            Self {
                is_standalone: need_sync_files,
                resp_tx,
                ctl_rx,
                shutdown_tx,
            },
            ControlPlaneRx {
                resp_rx,
                ctl_tx,
                shutdown_rx,
            },
        )
    }
}

impl ControlPlaneTx {
    fn need_sync_files(&self) -> bool {
        self.is_standalone
    }

    async fn sync_editor_changes(&mut self) {
        self.resp_ctl_plane(
            "SyncEditorChanges",
            ControlPlaneResponse::SyncEditorChanges(()),
        )
        .await;
    }

    async fn resp_ctl_plane(&mut self, loc: &str, resp: ControlPlaneResponse) -> bool {
        let sent = self.resp_tx.send(resp).is_ok();
        if !sent {
            log::warn!("failed to send {loc} response to editor");
        }

        sent
    }

    async fn next(&mut self) -> Option<ControlPlaneMessage> {
        self.ctl_rx.recv().await
    }
}

pub struct EditorActor<T> {
    client: Arc<T>,
    mailbox: mpsc::UnboundedReceiver<EditorActorRequest>,
    editor_conn: ControlPlaneTx,

    renderer_sender: broadcast::Sender<RenderActorRequest>,
    webview_sender: broadcast::Sender<WebviewActorRequest>,

    span_interner: SpanInterner,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event")]
pub enum ControlPlaneMessage {
    #[serde(rename = "changeCursorPosition")]
    ChangeCursorPosition(ChangeCursorPositionRequest),
    #[serde(rename = "panelScrollTo")]
    ResolveSourceLoc(ResolveSourceLocRequest),
    #[serde(rename = "panelScrollByPosition")]
    PanelScrollByPosition(PanelScrollByPositionRequest),
    #[serde(rename = "sourceScrollBySpan")]
    DocToSrcJumpResolve(DocToSrcJumpResolveRequest),
    #[serde(rename = "syncMemoryFiles")]
    SyncMemoryFiles(MemoryFiles),
    #[serde(rename = "updateMemoryFiles")]
    UpdateMemoryFiles(MemoryFiles),
    #[serde(rename = "removeMemoryFiles")]
    RemoveMemoryFiles(MemoryFilesShort),
}

#[derive(Debug, Serialize)]
#[serde(tag = "event")]
pub enum ControlPlaneResponse {
    #[serde(rename = "editorScrollTo")]
    EditorScrollTo(DocToSrcJumpInfo),
    #[serde(rename = "syncEditorChanges")]
    SyncEditorChanges(()),
    #[serde(rename = "compileStatus")]
    CompileStatus(CompileStatus),
    #[serde(rename = "outline")]
    Outline(Outline),
}

impl<T: EditorServer> EditorActor<T> {
    pub fn new(
        client: Arc<T>,
        mailbox: mpsc::UnboundedReceiver<EditorActorRequest>,
        editor_websocket_conn: ControlPlaneTx,
        renderer_sender: broadcast::Sender<RenderActorRequest>,
        webview_sender: broadcast::Sender<WebviewActorRequest>,
        span_interner: SpanInterner,
    ) -> Self {
        Self {
            client,
            mailbox,
            editor_conn: editor_websocket_conn,
            renderer_sender,
            webview_sender,

            span_interner,
        }
    }

    pub async fn run(mut self) {
        if self.editor_conn.need_sync_files() {
            self.editor_conn.sync_editor_changes().await;
        }

        loop {
            tokio::select! {
                Some(msg) = self.mailbox.recv() => {
                    log::trace!("EditorActor: received message from mailbox: {:?}", msg);
                   let sent = match msg {
                        EditorActorRequest::Shutdown => {
                            log::info!("EditorActor: received exit message");
                            break;
                        },
                        EditorActorRequest::DocToSrcJump(jump_info) => {
                            self.editor_conn.resp_ctl_plane("DocToSrcJump", ControlPlaneResponse::EditorScrollTo(jump_info)).await
                        },
                        EditorActorRequest::DocToSrcJumpResolve(req) => {
                            self.source_scroll_by_span(req.span)
                                .await;

                            false
                        },
                        EditorActorRequest::CompileStatus(status) => {
                            self.editor_conn.resp_ctl_plane("CompileStatus", ControlPlaneResponse::CompileStatus(status)).await
                        },
                        EditorActorRequest::Outline(outline) => {
                            self.editor_conn.resp_ctl_plane("Outline", ControlPlaneResponse::Outline(outline)).await
                        }
                    };

                    if !sent {
                        break;
                    }
                }
                Some(msg) = self.editor_conn.next() => {
                    match msg {
                        ControlPlaneMessage::ChangeCursorPosition(cursor_info) => {
                            log::debug!("EditorActor: received message from editor: {:?}", cursor_info);
                            self.renderer_sender.send(RenderActorRequest::ChangeCursorPosition(cursor_info)).log_error("EditorActor");
                        }
                        ControlPlaneMessage::ResolveSourceLoc(jump_info) => {
                            log::debug!("EditorActor: received message from editor: {:?}", jump_info);
                            self.renderer_sender.send(RenderActorRequest::ResolveSourceLoc(jump_info)).log_error("EditorActor");
                        }
                        ControlPlaneMessage::PanelScrollByPosition(jump_info) => {
                            log::debug!("EditorActor: received message from editor: {:?}", jump_info);
                            self.webview_sender.send(WebviewActorRequest::ViewportPosition(jump_info.position)).log_error("EditorActor");
                        }
                        ControlPlaneMessage::DocToSrcJumpResolve(jump_info) => {
                            log::debug!("EditorActor: received message from editor: {:?}", jump_info);

                            self.source_scroll_by_span(jump_info.span)
                                .await;
                        }
                        ControlPlaneMessage::SyncMemoryFiles(req) => {
                            log::debug!(
                                "EditorActor: processing SYNC memory files: {:?}",
                                req.files.keys().collect::<Vec<_>>()
                            );
                            handle_error(
                                "SyncMemoryFiles",
                                self.client.update_memory_files(req, true).await,
                            );
                        }
                        ControlPlaneMessage::UpdateMemoryFiles(req) => {
                            log::debug!(
                                "EditorActor: processing UPDATE memory files: {:?}",
                                req.files.keys().collect::<Vec<_>>()
                            );
                            handle_error(
                                "UpdateMemoryFiles",
                                self.client.update_memory_files(req, false).await,
                            );
                        }
                        ControlPlaneMessage::RemoveMemoryFiles(req) => {
                            log::debug!("EditorActor: processing REMOVE memory files: {:?}", req.files);
                            handle_error(
                                "RemoveMemoryFiles",
                                self.client.remove_shadow_files(req).await,
                            );
                        }
                    };
                }
            }
        }

        log::info!("EditorActor: editor disconnected");

        if self.editor_conn.is_standalone {
            log::info!("EditorActor: shutting down whole program");
            std::process::exit(0);
        }
    }

    async fn source_scroll_by_span(&mut self, span: String) {
        let jump_info = {
            match self.span_interner.span_by_str(&span).await {
                InternQuery::Ok(s) => s,
                InternQuery::UseAfterFree => {
                    log::warn!("EditorActor: out of date span id: {}", span);
                    return;
                }
            }
        };
        if let Some(span) = jump_info {
            let span_and_offset = span.into();
            self.renderer_sender
                .send(RenderActorRequest::EditorResolveSpanRange(
                    span_and_offset..span_and_offset,
                ))
                .log_error("EditorActor");
        };
    }
}

fn handle_error<T>(loc: &'static str, m: Result<T, reflexo_typst::Error>) -> Option<T> {
    if let Err(err) = &m {
        log::error!("EditorActor: failed to {loc}: {err:#}");
    }

    m.ok()
}
