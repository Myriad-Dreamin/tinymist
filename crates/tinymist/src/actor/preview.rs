use std::{collections::HashMap, sync::Arc};

use lsp_types::notification::Notification;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sync_lsp::{internal_error, LspClient, LspResult};
use tinymist_std::error::IgnoreLogging;
use tokio::sync::{mpsc, oneshot};
use typst_preview::{ControlPlaneMessage, Previewer};

use crate::{
    project::ProjectPreviewState,
    tool::preview::{HttpServer, PreviewProjectHandler},
};

pub struct PreviewTab {
    /// Task ID
    pub task_id: String,
    /// Previewer
    pub previewer: Previewer,
    /// Http Server for Previewer
    pub srv: HttpServer,
    /// Control plane message sender
    pub ctl_tx: mpsc::UnboundedSender<ControlPlaneMessage>,
    /// Compile handler
    pub compile_handler: Arc<PreviewProjectHandler>,
    /// Whether this tab is primary
    pub is_primary: bool,
}

pub enum PreviewRequest {
    Started(PreviewTab),
    Kill(String, oneshot::Sender<LspResult<JsonValue>>),
    Scroll(String, ControlPlaneMessage),
}

pub struct PreviewActor {
    pub client: LspClient,
    pub tabs: HashMap<String, PreviewTab>,
    pub preview_rx: mpsc::UnboundedReceiver<PreviewRequest>,
    /// the watchers for the preview
    pub(crate) watchers: ProjectPreviewState,
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

                    // Unregister preview early
                    let unregistered = self.watchers.unregister(&tab.compile_handler.project_id);
                    if !unregistered {
                        log::warn!("PreviewTask({task_id}): failed to unregister preview");
                    }

                    if tab.is_primary {
                        tab.compile_handler.unpin_primary();
                    } else {
                        tab.compile_handler.settle().log_error_with(|| {
                            format!("PreviewTask({}): failed to settle", tab.task_id)
                        });
                    }

                    let client = self.client.clone();
                    self.client.handle.spawn(async move {
                        tab.previewer.stop().await;
                        let _ = tab.srv.shutdown_tx.send(());

                        // Wait for previewer to stop
                        log::info!("PreviewTask({task_id}): wait for previewer to stop");
                        tab.previewer.join().await;
                        log::info!("PreviewTask({task_id}): wait for static server to stop");
                        let _ = tab.srv.join.await;

                        log::info!("PreviewTask({task_id}): killed");
                        // Send response
                        let _ = tx.send(Ok(JsonValue::Null));
                        // Send global notification
                        client.send_notification::<DisposePreview>(&DisposePreview { task_id });
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

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DisposePreview {
    task_id: String,
}

impl Notification for DisposePreview {
    type Params = Self;
    const METHOD: &'static str = "tinymist/preview/dispose";
}
