//! The actor that send notifications to the client.

use std::collections::HashMap;

use log::info;
use lsp_types::{Diagnostic, Url};
use tinymist_query::{DiagnosticsMap, LspDiagnostic};
use tokio::sync::mpsc;

use crate::{tools::word_count::WordsCount, LspHost, TypstLanguageServer};

pub enum EditorRequest {
    Diag(String, Option<DiagnosticsMap>),
    Status(String, TinymistCompileStatusEnum),
    WordCount(String, WordsCount),
}

pub struct EditorActor {
    host: LspHost<TypstLanguageServer>,
    editor_rx: mpsc::UnboundedReceiver<EditorRequest>,

    diagnostics: HashMap<Url, HashMap<String, Vec<LspDiagnostic>>>,
    affect_map: HashMap<String, Vec<Url>>,
    published_primary: bool,
    notify_compile_status: bool,
}

impl EditorActor {
    pub fn new(
        host: LspHost<TypstLanguageServer>,
        editor_rx: mpsc::UnboundedReceiver<EditorRequest>,
        notify_compile_status: bool,
    ) -> Self {
        Self {
            host,
            editor_rx,
            diagnostics: HashMap::new(),
            affect_map: HashMap::new(),
            published_primary: false,
            notify_compile_status,
        }
    }

    pub async fn run(mut self) {
        let mut compile_status = TinymistCompileStatusEnum::Compiling;
        let mut words_count = None;
        while let Some(req) = self.editor_rx.recv().await {
            match req {
                EditorRequest::Diag(group, diagnostics) => {
                    info!(
                        "received diagnostics from {group}: diag({:?})",
                        diagnostics.as_ref().map(|e| e.len())
                    );

                    let with_primary = self.affect_map.len() == 1
                        && self.affect_map.contains_key("primary")
                        && group == "primary";

                    self.publish(group, diagnostics, with_primary).await;

                    // Check with primary again after publish
                    let again_with_primary =
                        self.affect_map.len() == 1 && self.affect_map.contains_key("primary");

                    if !with_primary && self.published_primary != again_with_primary {
                        self.flush_primary_diagnostics(again_with_primary).await;
                        self.published_primary = again_with_primary;
                    }
                }
                EditorRequest::Status(group, status) => {
                    log::debug!("received status request");
                    if self.notify_compile_status && group == "primary" {
                        compile_status = status;
                        self.host.send_notification::<TinymistCompileStatus>(
                            TinymistCompileStatus {
                                status: compile_status.clone(),
                                words_count: words_count.clone(),
                            },
                        );
                    }
                }
                EditorRequest::WordCount(group, wc) => {
                    log::debug!("received word count request");
                    if self.notify_compile_status && group == "primary" {
                        words_count = Some(wc);
                        self.host.send_notification::<TinymistCompileStatus>(
                            TinymistCompileStatus {
                                status: compile_status.clone(),
                                words_count: words_count.clone(),
                            },
                        );
                    }
                }
            }
        }
        info!("compile cluster actor is stopped");
    }

    async fn flush_primary_diagnostics(&mut self, enable: bool) {
        let affected = self.affect_map.get("primary");

        for url in affected.into_iter().flatten() {
            let path_diags = self.diagnostics.get(url);

            let diags = path_diags.into_iter().flatten();
            let diags = diags.filter_map(|(g, diags)| (g != "primary" || enable).then_some(diags));
            let to_publish = diags.flatten().cloned().collect();

            self.host.publish_diagnostics(url.clone(), to_publish, None);
        }
    }

    pub async fn publish(
        &mut self,
        group: String,
        next_diag: Option<DiagnosticsMap>,
        with_primary: bool,
    ) {
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
                self.publish_inner(&group, with_primary, url, None)
            }
        }

        // Get touched updates
        for (url, next) in next_diag.into_iter().flatten() {
            self.publish_inner(&group, with_primary, url, Some(next))
        }
    }

    fn publish_inner(
        &mut self,
        group: &str,
        with_primary: bool,
        url: Url,
        next: Option<Vec<Diagnostic>>,
    ) {
        let mut to_publish = Vec::new();

        // Get the diagnostics from other groups
        let path_diags = self.diagnostics.entry(url.clone()).or_default();
        for (g, diags) in &*path_diags {
            if (with_primary || g != "primary") && g != group {
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

        if group != "primary" || with_primary {
            self.host.publish_diagnostics(url, to_publish, None)
        }
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
pub struct TinymistCompileStatus {
    pub status: TinymistCompileStatusEnum,
    pub words_count: Option<WordsCount>,
}

impl lsp_types::notification::Notification for TinymistCompileStatus {
    type Params = Self;
    const METHOD: &'static str = "tinymist/compileStatus";
}
