use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use lsp_types::request::Request;
use reflexo::ImmutPath;
use reflexo_typst::vfs::{FileChangeSet, PathAccessModel};
use reflexo_typst::Bytes;
use serde::{Deserialize, Serialize};
use sync_ls::TypedLspClient;
use tinymist_project::{Interrupt, FILE_MISSING_ERROR};
use typst::diag::FileResult;

use crate::vfs::notify::NotifyMessage;
use crate::ServerState;

impl ServerState {
    /// Handles the dependency changes.
    pub(crate) fn handle_deps(&mut self) {
        let Some(dep_rx) = self.dep_rx.as_mut() else {
            return;
        };

        while let Ok(msg) = dep_rx.try_recv() {
            match msg {
                NotifyMessage::Settle => {}
                NotifyMessage::SyncDependency(notify_deps) => {
                    let mut deps = HashSet::new();
                    // todo: poor performance
                    notify_deps.dependencies(&mut |path| {
                        deps.insert(path.clone());
                    });

                    let am = self.config.watch_access_model(&self.client);
                    am.retain_watch(|path| deps.contains(path));
                }
                NotifyMessage::UpstreamUpdate(evt) => {
                    let changeset = FileChangeSet::new_inserts(vec![]);
                    self.project.interrupt(Interrupt::Fs(
                        crate::vfs::FilesystemEvent::UpstreamUpdate {
                            changeset,
                            upstream_event: Some(evt),
                        },
                    ));
                }
            }
        }
    }
}

/// The access model that watches the file content from the client.
#[derive(Clone)]
pub struct WatchAccessModel {
    pub watches: Arc<Mutex<HashSet<ImmutPath>>>,
    pub client: TypedLspClient<ServerState>,
}

impl WatchAccessModel {
    pub fn new(client: TypedLspClient<ServerState>) -> Self {
        Self {
            client,
            watches: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub fn add_watch(&self, path: &Path) {
        log::info!("add watch: {path:?}");
        let req = FsWatchRequest {
            inserts: vec![tinymist_query::path_to_url(path).unwrap()],
            removes: vec![],
        };
        self.client
            .send_lsp_request::<FsWatchRequest>(req, |_stat, resp| {
                if let Some(e) = resp.result {
                    log::error!("Failed to watch file: {e:?}");
                }
            });

        let path = ImmutPath::from(path);
        let mut watches = self.watches.lock().unwrap();
        watches.insert(path);
    }

    pub fn retain_watch(&self, filter: impl Fn(&Path) -> bool) {
        let mut removes = vec![];

        let mut watches = self.watches.lock().unwrap();
        watches.retain(|path| {
            if !filter(path) {
                // todo: clone here
                removes.push(tinymist_query::path_to_url(path.as_ref()).unwrap());
                return false;
            }

            true
        });
        drop(watches);

        let req = FsWatchRequest {
            inserts: vec![],
            removes,
        };
        self.client
            .send_lsp_request::<FsWatchRequest>(req, |_stat, resp| {
                if let Some(e) = resp.result {
                    log::error!("Failed to watch file: {e:?}");
                }
            });
    }
}

impl PathAccessModel for WatchAccessModel {
    fn content(&self, src: &Path) -> FileResult<Bytes> {
        // Requests to load the file content from the client.
        self.add_watch(src);
        // Returns an error to indicate that the file is not available locally.
        Err(FILE_MISSING_ERROR.clone())
    }
}

/// The file content request for the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateFileContent {
    // default encoding is base64
    content: String,
}

/// The file system watch request for the client.
/// This is used to watch the file content from the client.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsWatchRequest {
    inserts: Vec<lsp_types::Url>,
    removes: Vec<lsp_types::Url>,
}

impl Request for FsWatchRequest {
    type Params = Self;
    type Result = ();
    const METHOD: &'static str = "tinymist/fs/watch";
}
