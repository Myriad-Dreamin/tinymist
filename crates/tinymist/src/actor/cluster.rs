//! The cluster actor running in background

use std::collections::HashMap;

use log::info;
use lsp_types::Url;
use tinymist_query::{DiagnosticsMap, LspDiagnostic};
use tokio::sync::mpsc;

use crate::{tools::word_count::WordsCount, LspHost, TypstLanguageServer};

pub enum CompileClusterRequest {
    Diag(String, Option<DiagnosticsMap>),
    Status(String, TinymistCompileStatusEnum),
    WordCount(String, Option<WordsCount>),
}

pub struct EditorActor {
    pub host: LspHost<TypstLanguageServer>,
    pub diag_rx: mpsc::UnboundedReceiver<CompileClusterRequest>,

    pub diagnostics: HashMap<Url, HashMap<String, Vec<LspDiagnostic>>>,
    pub affect_map: HashMap<String, Vec<Url>>,
    pub published_primary: bool,
    pub notify_compile_status: bool,
}

impl EditorActor {
    pub async fn run(mut self) {
        let mut compile_status = TinymistCompileStatusEnum::Compiling;
        let mut words_count = None;
        loop {
            tokio::select! {
                e = self.diag_rx.recv() => {
                    match e {
                        Some(CompileClusterRequest::Diag(group, diagnostics)) => {
                            info!("received diagnostics from {}: diag({:?})", group, diagnostics.as_ref().map(|e| e.len()));

                            let with_primary = (self.affect_map.len() <= 1 && self.affect_map.contains_key("primary")) && group == "primary";

                            self.publish(group, diagnostics, with_primary).await;

                            // Check with primary again after publish
                            let again_with_primary = self.affect_map.len() == 1 && self.affect_map.contains_key("primary");

                            if !with_primary && self.published_primary != again_with_primary {
                                self.flush_primary_diagnostics(again_with_primary).await;
                                self.published_primary = again_with_primary;
                            }
                        }
                        Some(CompileClusterRequest::Status(group, status)) => {
                            log::debug!("received status request");
                            if self.notify_compile_status {
                                if group != "primary" {
                                  continue;
                                }
                                compile_status = status;
                                self.host.send_notification::<TinymistCompileStatus>(TinymistCompileStatus {
                                    status: compile_status.clone(),
                                    words_count: words_count.clone(),
                                });
                            }
                        }
                        Some(CompileClusterRequest::WordCount(group, wc)) => {
                            log::debug!("received word count request");
                            if self.notify_compile_status {
                                if group != "primary" {
                                continue;
                                }
                                words_count = wc;
                                self.host.send_notification::<TinymistCompileStatus>(TinymistCompileStatus {
                                    status: compile_status.clone(),
                                    words_count: words_count.clone(),
                                });
                            }
                        }
                        None => {
                            break;
                        }
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

            let diags = path_diags.into_iter().flatten().filter_map(|(g, diags)| {
                if g == "primary" {
                    return enable.then_some(diags);
                }
                Some(diags)
            });
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
        let is_primary = group == "primary";
        let clear_all = next_diag.is_none();
        let affect_list = next_diag.as_ref().map(|e| e.keys().cloned().collect());
        let affect_list: Vec<_> = affect_list.unwrap_or_default();

        // Get sources which had some diagnostic published last time, but not this time.
        //
        // The LSP specifies that files will not have diagnostics updated, including
        // removed, without an explicit update, so we need to send an empty `Vec` of
        // diagnostics to these sources.

        // Get sources that affected by this group in last round but not this time
        let affected = self.affect_map.get_mut(&group).map(std::mem::take);
        let affected = affected.into_iter().flatten().map(|e| (e, None));
        let prev_aff: Vec<_> = if let Some(n) = next_diag.as_ref() {
            affected.filter(|e| !n.contains_key(&e.0)).collect()
        } else {
            affected.collect()
        };

        // Get touched updates
        let next_aff = next_diag.into_iter().flatten().map(|(x, y)| (x, Some(y)));

        let tasks = prev_aff.into_iter().chain(next_aff);
        for (url, next) in tasks {
            // Get the diagnostics from other groups
            let path_diags = self.diagnostics.entry(url.clone()).or_default();
            let rest_all = path_diags
                .iter()
                .filter_map(|(g, diags)| {
                    if (!with_primary && g == "primary") || g == &group {
                        return None;
                    }

                    Some(diags)
                })
                .flatten()
                .cloned();

            // Get the diagnostics from this group
            let next_all = next.clone().into_iter().flatten();
            let to_publish = rest_all.chain(next_all).collect();

            match next {
                Some(next) => {
                    path_diags.insert(group.clone(), next);
                }
                None => {
                    path_diags.remove(&group);
                }
            }

            if !is_primary || with_primary {
                self.host.publish_diagnostics(url, to_publish, None)
            }
        }

        if clear_all {
            // We just used the cache, and won't need it again, so we can update it now
            self.affect_map.remove(&group);
        } else {
            // We just used the cache, and won't need it again, so we can update it now
            self.affect_map.insert(group, affect_list);
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
pub struct TinymistCompileStatus {
    pub status: TinymistCompileStatusEnum,
    #[serde(rename = "wordsCount")]
    pub words_count: Option<WordsCount>,
}

impl lsp_types::notification::Notification for TinymistCompileStatus {
    type Params = TinymistCompileStatus;
    const METHOD: &'static str = "tinymist/compileStatus";
}
