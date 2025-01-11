//! The actor maintain output to the editor, including diagnostics and compile
//! status.

use std::collections::HashMap;

use log::info;
use lsp_types::{notification::PublishDiagnostics, Diagnostic, PublishDiagnosticsParams, Url};
use tinymist_query::DiagnosticsMap;
use tokio::sync::mpsc;

use crate::{tool::word_count::WordsCount, LspClient};

pub struct DocVersion {
    pub group: String,
    pub revision: usize,
}

#[derive(Debug, Clone)]
pub struct CompileStatus {
    pub group: String,
    pub path: String,
    pub status: TinymistCompileStatusEnum,
}

pub enum EditorRequest {
    Diag(DocVersion, Option<DiagnosticsMap>),
    Status(CompileStatus),
    WordCount(String, WordsCount),
}

pub struct EditorActor {
    client: LspClient,
    editor_rx: mpsc::UnboundedReceiver<EditorRequest>,

    diagnostics: HashMap<Url, HashMap<String, Vec<Diagnostic>>>,
    affect_map: HashMap<String, Vec<Url>>,
    notify_compile_status: bool,
}

impl EditorActor {
    pub fn new(
        client: LspClient,
        editor_rx: mpsc::UnboundedReceiver<EditorRequest>,
        notify_compile_status: bool,
    ) -> Self {
        Self {
            client,
            editor_rx,
            diagnostics: HashMap::new(),
            affect_map: HashMap::new(),
            notify_compile_status,
        }
    }

    pub async fn run(mut self) {
        let mut compile_status = CompileStatus {
            group: "primary".to_owned(),
            status: TinymistCompileStatusEnum::Compiling,
            path: "".to_owned(),
        };
        let mut words_count = None;
        while let Some(req) = self.editor_rx.recv().await {
            match req {
                EditorRequest::Diag(dv, diagnostics) => {
                    let DocVersion { group, revision } = dv;
                    info!(
                        "received diagnostics from {group}:{revision}: diag({:?})",
                        diagnostics.as_ref().map(|e| e.len())
                    );

                    self.publish(group, diagnostics).await;
                }
                EditorRequest::Status(status) => {
                    log::info!("received status request({status:?})");
                    if self.notify_compile_status && status.group == "primary" {
                        compile_status = status;
                        self.client.send_notification::<TinymistCompileStatus>(
                            TinymistCompileStatus {
                                status: compile_status.status.clone(),
                                path: compile_status.path.clone(),
                                words_count: words_count.clone(),
                            },
                        );
                    }
                }
                EditorRequest::WordCount(group, wc) => {
                    log::debug!("received word count request");
                    if self.notify_compile_status && group == "primary" {
                        words_count = Some(wc);
                        self.client.send_notification::<TinymistCompileStatus>(
                            TinymistCompileStatus {
                                status: compile_status.status.clone(),
                                path: compile_status.path.clone(),
                                words_count: words_count.clone(),
                            },
                        );
                    }
                }
            }
        }
        info!("compile cluster actor is stopped");
    }

    pub async fn publish(&mut self, group: String, next_diag: Option<DiagnosticsMap>) {
        let affected = match next_diag.as_ref() {
            Some(e) => self
                .affect_map
                .insert(group.clone(), e.keys().cloned().collect()),
            None => self.affect_map.remove(&group),
        };

        // Get sources which had some diagnostic published last time, but not this time.
        //
        // The LSP specifies that files will not have diagnostics updated, including
        // removed, without an explicit update, so we need to send an empty `Vec` of
        // diagnostics to these sources.

        // Get sources that affected by this group in last round but not this time
        for url in affected.into_iter().flatten() {
            if !next_diag.as_ref().is_some_and(|e| e.contains_key(&url)) {
                self.publish_inner(&group, url, None)
            }
        }

        // Get touched updates
        for (url, next) in next_diag.into_iter().flatten() {
            self.publish_inner(&group, url, Some(next))
        }
    }

    fn publish_inner(&mut self, group: &str, url: Url, next: Option<Vec<Diagnostic>>) {
        let mut to_publish = Vec::new();

        // Get the diagnostics from other groups
        let path_diags = self.diagnostics.entry(url.clone()).or_default();
        for (g, diags) in &*path_diags {
            if g != group {
                to_publish.extend(diags.iter().cloned());
            }
        }

        // Get the diagnostics from this group
        if let Some(diags) = &next {
            to_publish.extend(diags.iter().cloned())
        }

        match next {
            Some(next) => path_diags.insert(group.to_owned(), next),
            None => path_diags.remove(group),
        };

        self.client
            .send_notification::<PublishDiagnostics>(PublishDiagnosticsParams {
                uri: url,
                diagnostics: to_publish,
                version: None,
            });
    }
}
// Notification

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TinymistCompileStatusEnum {
    Compiling,
    CompileSuccess,
    CompileError,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TinymistCompileStatus {
    pub status: TinymistCompileStatusEnum,
    pub path: String,
    pub words_count: Option<WordsCount>,
}

impl lsp_types::notification::Notification for TinymistCompileStatus {
    type Params = Self;
    const METHOD: &'static str = "tinymist/compileStatus";
}
