//! The cluster actor running in background

use std::collections::HashMap;

use futures::future::join_all;
use log::info;
use lsp_types::{Diagnostic, Url};
use tinymist_query::{DiagnosticsMap, LspDiagnostic};
use tokio::sync::mpsc;

use crate::LspHost;

pub struct CompileClusterActor {
    pub host: LspHost,
    pub diag_rx: mpsc::UnboundedReceiver<(String, Option<DiagnosticsMap>)>,

    pub diagnostics: HashMap<Url, HashMap<String, Vec<LspDiagnostic>>>,
    pub affect_map: HashMap<String, Vec<Url>>,
    pub published_primary: bool,
}

impl CompileClusterActor {
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                e = self.diag_rx.recv() => {
                    let Some((group, diagnostics)) = e else {
                        break;
                    };
                    info!("received diagnostics from {}: diag({:?})", group, diagnostics.as_ref().map(|e| e.len()));
                    let with_primary = (self.affect_map.len() <= 1 && self.affect_map.contains_key("primary")) && group == "primary";
                    self.publish(group, diagnostics, with_primary).await;
                    if !with_primary {
                        let again_with_primary = self.affect_map.len() == 1 && self.affect_map.contains_key("primary");
                        if self.published_primary != again_with_primary {
                            self.flush_primary_diagnostics(again_with_primary).await;
                            self.published_primary = again_with_primary;
                        }
                    }
                }
            }
            info!("compile cluster actor is stopped");
        }
    }

    pub async fn do_publish_diagnostics(
        host: &LspHost,
        uri: Url,
        diags: Vec<Diagnostic>,
        version: Option<i32>,
        ignored: bool,
    ) {
        if ignored {
            return;
        }

        host.publish_diagnostics(uri, diags, version)
    }

    async fn flush_primary_diagnostics(&mut self, enable: bool) {
        let affected = self.affect_map.get("primary");

        let tasks = affected.into_iter().flatten().map(|url| {
            let path_diags = self.diagnostics.get(url);

            let diags = path_diags.into_iter().flatten().filter_map(|(g, diags)| {
                if g == "primary" {
                    return enable.then_some(diags);
                }
                Some(diags)
            });
            // todo: .flatten() removed
            // let to_publish = diags.flatten().cloned().collect();
            let to_publish = diags.flatten().cloned().collect();

            Self::do_publish_diagnostics(&self.host, url.clone(), to_publish, None, false)
        });

        join_all(tasks).await;
    }

    pub async fn publish(
        &mut self,
        group: String,
        next_diagnostics: Option<DiagnosticsMap>,
        with_primary: bool,
    ) {
        let is_primary = group == "primary";

        let affected = self.affect_map.get_mut(&group);

        let affected = affected.map(std::mem::take);

        // Gets sources which had some diagnostic published last time, but not this
        // time. The LSP specifies that files will not have diagnostics
        // updated, including removed, without an explicit update, so we need
        // to send an empty `Vec` of diagnostics to these sources.
        // todo: merge
        let clear_list = if let Some(n) = next_diagnostics.as_ref() {
            affected
                .into_iter()
                .flatten()
                .filter(|e| !n.contains_key(e))
                .map(|e| (e, None))
                .collect::<Vec<_>>()
        } else {
            affected
                .into_iter()
                .flatten()
                .map(|e| (e, None))
                .collect::<Vec<_>>()
        };
        let next_affected = if let Some(n) = next_diagnostics.as_ref() {
            n.keys().cloned().collect()
        } else {
            Vec::new()
        };
        let clear_all = next_diagnostics.is_none();
        // Gets touched updates
        let update_list = next_diagnostics
            .into_iter()
            .flatten()
            .map(|(x, y)| (x, Some(y)));

        let tasks = clear_list.into_iter().chain(update_list);
        let tasks = tasks.map(|(url, next)| {
            let path_diags = self.diagnostics.entry(url.clone()).or_default();
            let rest_all = path_diags
                .iter()
                .filter_map(|(g, diags)| {
                    if !with_primary && g == "primary" {
                        return None;
                    }
                    if g != &group {
                        Some(diags)
                    } else {
                        None
                    }
                })
                .flatten()
                .cloned();

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

            Self::do_publish_diagnostics(
                &self.host,
                url,
                to_publish,
                None,
                is_primary && !with_primary,
            )
        });

        join_all(tasks).await;

        if clear_all {
            // We just used the cache, and won't need it again, so we can update it now
            self.affect_map.remove(&group);
        } else {
            // We just used the cache, and won't need it again, so we can update it now
            self.affect_map.insert(group, next_affected);
        }
    }
}
