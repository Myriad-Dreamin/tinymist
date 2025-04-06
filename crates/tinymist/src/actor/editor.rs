//! The actor maintaining output to the editor, including diagnostics and
//! compile status.

use std::collections::HashMap;

use lsp_types::notification::{Notification, PublishDiagnostics as PublishDiagnosticsBase};
use lsp_types::{Diagnostic, Url};
use reflexo_typst::typst::prelude::{eco_vec, EcoVec};
use serde::{Deserialize, Serialize};
use tinymist_query::DiagnosticsMap;
use tokio::sync::mpsc;

use crate::project::ProjectInsId;
use crate::{tool::word_count::WordsCount, LspClient};

#[derive(Debug, Clone)]
pub struct EditorActorConfig {
    /// Whether to notify status to the editor.
    pub notify_status: bool,
}

/// The request to the editor actor.
pub enum EditorRequest {
    Config(EditorActorConfig),
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
    /// The configuration of the editor actor.
    config: EditorActorConfig,

    /// Accumulated diagnostics per file.
    /// The outer `HashMap` is indexed by the file's URL.
    /// The inner `HashMap` is indexed by the project ID, allowing multiple
    /// projects publishing diagnostics to the same file independently.
    diagnostics: HashMap<Url, HashMap<ProjectInsId, EcoVec<Diagnostic>>>,
    /// The map from project ID to the affected files.
    affect_map: HashMap<ProjectInsId, Vec<Url>>,
}

impl EditorActor {
    /// Creates a new editor actor.
    pub fn new(
        client: LspClient,
        editor_rx: mpsc::UnboundedReceiver<EditorRequest>,
        notify_status: bool,
    ) -> Self {
        Self {
            client,
            editor_rx,
            diagnostics: HashMap::new(),
            affect_map: HashMap::new(),
            config: EditorActorConfig { notify_status },
        }
    }

    /// Runs the editor actor in background. It exits when the editor channel
    /// is closed.
    pub async fn run(mut self) {
        // The local state.
        let mut status = StatusAll {
            status: CompileStatusEnum::Compiling,
            path: "".to_owned(),
            words_count: None,
        };

        while let Some(req) = self.editor_rx.recv().await {
            match req {
                EditorRequest::Config(config) => {
                    log::info!("received config request: {config:?}");
                    self.config = config;
                }
                EditorRequest::Diag(version, diagnostics) => {
                    log::debug!(
                        "received diagnostics from {version:?}: diag({:?})",
                        diagnostics.as_ref().map(|files| files.len())
                    );

                    self.publish(version.id, diagnostics).await;
                }
                EditorRequest::Status(compile_status) => {
                    log::trace!("received status request: {compile_status:?}");
                    if self.config.notify_status && compile_status.id == ProjectInsId::PRIMARY {
                        status.status = compile_status.status;
                        status.path = compile_status.path;
                        self.client.send_notification::<StatusAll>(&status);
                    }
                }
                EditorRequest::WordCount(id, count) => {
                    log::trace!("received word count request");
                    if self.config.notify_status && id == ProjectInsId::PRIMARY {
                        status.words_count = Some(count);
                        self.client.send_notification::<StatusAll>(&status);
                    }
                }
            }
        }

        log::info!("editor actor is stopped");
    }

    /// Publishes diagnostics of a project to the editor.
    pub async fn publish(&mut self, id: ProjectInsId, next_diag: Option<DiagnosticsMap>) {
        let affected = match next_diag.as_ref() {
            Some(next_diag) => self
                .affect_map
                .insert(id.clone(), next_diag.keys().cloned().collect()),
            None => self.affect_map.remove(&id),
        };

        // Gets sources which had some diagnostic published last time, but not this
        // time.
        //
        // The LSP specifies that files will not have diagnostics updated, including
        // removed, without an explicit update, so we need to send an empty `Vec` of
        // diagnostics to these sources.

        // Gets sources that affected by this group in last round but not this time
        for uri in affected.into_iter().flatten() {
            if !next_diag.as_ref().is_some_and(|e| e.contains_key(&uri)) {
                self.publish_file(&id, uri, None)
            }
        }

        // Gets touched updates
        for (uri, next) in next_diag.into_iter().flatten() {
            self.publish_file(&id, uri, Some(next))
        }
    }

    /// Publishes diagnostics of a file to the editor.
    fn publish_file(&mut self, id: &ProjectInsId, uri: Url, next: Option<EcoVec<Diagnostic>>) {
        let mut diagnostics = EcoVec::new();

        // Gets the diagnostics from other groups
        let path_diags = self.diagnostics.entry(uri.clone()).or_default();
        for (existing_id, diags) in path_diags.iter() {
            if existing_id != id {
                diagnostics.push(diags.clone());
            }
        }

        // Gets the diagnostics from this group
        if let Some(diags) = &next {
            diagnostics.push(diags.clone())
        }

        // Updates the diagnostics for this group
        match next {
            Some(next) => path_diags.insert(id.clone(), next),
            None => path_diags.remove(id),
        };

        // Publishes the diagnostics
        self.client
            .send_notification::<PublishDiagnostics>(&PublishDiagnosticsParams {
                uri,
                diagnostics: ScatterVec(diagnostics),
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

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
pub struct PublishDiagnosticsParams {
    /// The URI for which diagnostic information is reported.
    pub uri: Url,

    /// An array of diagnostic information items.
    pub diagnostics: ScatterVec<Diagnostic>,

    /// Optional the version number of the document the diagnostics are
    /// published for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i32>,
}

/// Diagnostics notification are sent from the server to the client to signal
/// results of validation runs.
#[derive(Debug)]
pub enum PublishDiagnostics {}

impl Notification for PublishDiagnostics {
    type Params = PublishDiagnosticsParams;
    const METHOD: &'static str = PublishDiagnosticsBase::METHOD;
}

/// A scatter vector that is serialized as a flatten representation.
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct ScatterVec<T>(EcoVec<EcoVec<T>>);

impl serde::Serialize for ScatterVec<Diagnostic> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut vec = Vec::new();
        for e in &self.0 {
            vec.extend(e.iter().cloned())
        }
        vec.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for ScatterVec<Diagnostic> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let vec = EcoVec::<Diagnostic>::deserialize(deserializer)?;
        Ok(ScatterVec(eco_vec![vec]))
    }
}
