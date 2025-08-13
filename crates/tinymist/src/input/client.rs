use std::path::{Path, PathBuf};

use base64::Engine;
use lsp_types::request::Request;
use reflexo_typst::{vfs::PathAccessModel, Bytes};
use serde::{Deserialize, Serialize};
use sync_ls::TypedLspClient;
use typst::diag::{FileError, FileResult};

use crate::ServerState;

/// Provides ClientAccessModel that only accesses file system via client APIs.
#[derive(Clone)]
pub struct ClientAccessModel {
    pub client: TypedLspClient<ServerState>,
}

impl ClientAccessModel {
    pub fn new(client: TypedLspClient<ServerState>) -> Self {
        Self { client }
    }
}

impl PathAccessModel for ClientAccessModel {
    fn content(&self, src: &Path) -> FileResult<Bytes> {
        log::info!("Requesting file content for {src:?}");
        #[cfg(feature = "web")]
        {
            // let (tx, rx) = tokio::sync::oneshot::channel();
            // client.send_lsp_request::<FsReadRequest>(req, |_stat, resp| {
            //     let res = tx.send(resp);
            //     if let Err(e) = res {
            //         log::error!("Failed to send response for file stat request:
            // {e:?}");     }
            // });

            let res = self
                .client
                .content(src)
                .and_then(|res| {
                    base64::engine::general_purpose::STANDARD
                        .decode(res.content)
                        .map_err(|err| {
                            std::io::Error::other(format!("Failed to decode file content: {err}"))
                        })
                        .map(Bytes::new)
                })
                .map_err(|e| FileError::from_io(e, src));
            log::info!("Requested file content for {src:?} => {res:?}");
            res
        }
        #[cfg(not(feature = "web"))]
        {
            todo!()
        }
    }
}

/// The file content request for the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegateFileContent {
    // default encoding is base64
    content: String,
}

/// The file system read request for the client.
/// This is used to read the file content from the client.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsReadRequest {
    path: PathBuf,
}

impl Request for FsReadRequest {
    type Params = Self;
    type Result = DelegateFileContent;
    const METHOD: &'static str = "tinymist/fs/content";
}
