//! The actor maintaining output to the editor, including diagnostics and
//! compile status.

use std::collections::HashMap;

use lsp_types::{notification::PublishDiagnostics, Diagnostic, PublishDiagnosticsParams, Url};
use tinymist_project::ProjectInsId;
use tinymist_query::DiagnosticsMap;
use tokio::sync::mpsc;

use crate::{tool::word_count::WordsCount, LspClient};

/// The request to the editor actor.
pub enum EditorRequest {
    /// Publishes diagnostics to the editor.
    Diag(ProjVersion, Option<DiagnosticsMap>),
    /// Updates compile status to the editor.
    Status(CompileStatus),
    /// Updastes words count status to the editor.
    WordCount(ProjectInsId, WordsCount),
}

/// The actor maintaining output to the editor, including diagnostics and
/// compile status.
pub struct EditorActor {
    /// The connection to the lsp client.
    client: LspClient,
    /// The channel receiving the [`EditorRequest`].
    editor_rx: mpsc::UnboundedReceiver<EditorRequest>,
    /// Whether to notify compile status to the editor.
    notify_compile_status: bool,

    /// Accumulated diagnostics per file.
    /// The outer `HashMap` is indexed by the file's URL.
    /// The inner `HashMap` is indexed by the project ID, allowing multiple
    /// projects publishing diagnostics to the same file independently.
    diagnostics: HashMap<Url, HashMap<ProjectInsId, Vec<Diagnostic>>>,
    /// The map from project ID to the affected files.
    affect_map: HashMap<ProjectInsId, Vec<Url>>,
}

impl EditorActor {
    /// Creates a new editor actor.
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

    /// Runs the editor actor in background. It exits when the editor channel
    /// is closed.
    pub async fn run(mut self) {
        // The local state.
        let mut compile_status = StatusAll {
            status: CompileStatusEnum::Compiling,
            path: "".to_owned(),
            words_count: None,
        };

        while let Some(req) = self.editor_rx.recv().await {
            match req {
                EditorRequest::Diag(dv, diagnostics) => {
                    log::debug!(
                        "received diagnostics from {dv:?}: diag({:?})",
                        diagnostics.as_ref().map(|e| e.len())
                    );

                    self.publish(dv.id, diagnostics).await;
                }
                EditorRequest::Status(status) => {
                    log::debug!("received status request({status:?})");
                    if self.notify_compile_status && status.id == ProjectInsId::PRIMARY {
                        compile_status.status = status.status;
                        compile_status.path = status.path;
                        self.client.send_notification::<StatusAll>(&compile_status);
                    }
                }
                EditorRequest::WordCount(id, count) => {
                    log::debug!("received word count request");
                    if self.notify_compile_status && id == ProjectInsId::PRIMARY {
                        compile_status.words_count = Some(count);
                        self.client.send_notification::<StatusAll>(&compile_status);
                    }
                }
            }
        }

        log::info!("editor actor is stopped");
    }

    /// Publishes diagnostics of a project to the editor.
    pub async fn publish(&mut self, id: ProjectInsId, next_diag: Option<DiagnosticsMap>) {
        let affected = match next_diag.as_ref() {
            Some(e) => self
                .affect_map
                .insert(id.clone(), e.keys().cloned().collect()),
            None => self.affect_map.remove(&id),
        };

        // Gets sources which had some diagnostic published last time, but not this
        // time.
        //
        // The LSP specifies that files will not have diagnostics updated, including
        // removed, without an explicit update, so we need to send an empty `Vec` of
        // diagnostics to these sources.

        // Gets sources that affected by this group in last round but not this time
        for url in affected.into_iter().flatten() {
            if !next_diag.as_ref().is_some_and(|e| e.contains_key(&url)) {
                self.publish_file(&id, url, None)
            }
        }

        // Gets touched updates
        for (url, next) in next_diag.into_iter().flatten() {
            self.publish_file(&id, url, Some(next))
        }
    }

    /// Publishes diagnostics of a file to the editor.
    fn publish_file(&mut self, group: &ProjectInsId, uri: Url, next: Option<Vec<Diagnostic>>) {
        let mut diagnostics = Vec::new();

        // Gets the diagnostics from other groups
        let path_diags = self.diagnostics.entry(uri.clone()).or_default();
        for (g, diags) in path_diags.iter() {
            if g != group {
                diagnostics.extend(diags.iter().cloned());
            }
        }

        // Gets the diagnostics from this group
        if let Some(diags) = &next {
            diagnostics.extend(diags.iter().cloned())
        }

        match next {
            Some(next) => path_diags.insert(group.clone(), next),
            None => path_diags.remove(group),
        };

        self.client
            .send_notification::<PublishDiagnostics>(&PublishDiagnosticsParams {
                uri,
                diagnostics,
                version: None,
            });
    }
}

/// The compilation revision of a project.
#[derive(Debug, Clone)]
pub struct ProjVersion {
    /// The project ID.
    pub id: ProjectInsId,
    /// The revision of the project (compilation).
    pub revision: usize,
}

/// The compilation status of a project.
#[derive(Debug, Clone)]
pub struct CompileStatus {
    /// The project ID.
    pub id: ProjectInsId,
    /// The file getting compiled.
    // todo: eco string
    pub path: String,
    /// The status of the compilation.
    pub status: CompileStatusEnum,
}

/// The compilation status of a project.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompileStatusEnum {
    /// The project is compiling.
    Compiling,
    /// The project compiled successfully.
    CompileSuccess,
    /// The project failed to compile.
    CompileError,
}

/// All the status of a project.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusAll {
    /// The status of the project.
    pub status: CompileStatusEnum,
    /// The file getting compiled.
    pub path: String,
    /// The word count of the project.
    pub words_count: Option<WordsCount>,
}

impl lsp_types::notification::Notification for StatusAll {
    type Params = Self;
    const METHOD: &'static str = "tinymist/compileStatus";
}
