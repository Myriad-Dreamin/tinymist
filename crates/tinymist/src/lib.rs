//! # tinymist
//!
//! This crate provides a CLI that starts services for [Typst](https://typst.app/). It provides:
//! + `tinymist lsp`: A language server following the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/).
//! + `tinymist preview`: A preview server for Typst.
//!
//! ## Usage
//!
//! See [Features: Command Line Interface](https://myriad-dreamin.github.io/tinymist/feature/cli.html).
//!
//! ## Documentation
//!
//! See [Crate Docs](https://myriad-dreamin.github.io/tinymist/rs/tinymist/index.html).
//!
//! Also see [Developer Guide: Tinymist LSP](https://myriad-dreamin.github.io/tinymist/module/lsp.html).
//!
//! ## Contributing
//!
//! See [CONTRIBUTING.md](https://github.com/Myriad-Dreamin/tinymist/blob/main/CONTRIBUTING.md).

mod actor;
mod cmd;
mod init;
mod resource;
mod server;
mod stats;
mod task;
use futures::future::MaybeDone;
pub use task::UserActionTask;
pub mod tool;
mod utils;

pub use init::*;
pub use server::*;
pub use sync_lsp::LspClient;
pub use tinymist_query as query;
pub use tinymist_world as world;
pub use world::*;

use lsp_server::{RequestId, ResponseError};
use serde_json::from_value;
use sync_lsp::*;
use utils::*;

use tinymist_query::CompilerQueryResponse;

/// The future type for a lsp query.
pub type QueryFuture = anyhow::Result<ResponseFuture<anyhow::Result<CompilerQueryResponse>>>;

trait LspClientExt {
    fn schedule_query(&self, req_id: RequestId, query_fut: QueryFuture) -> ScheduledResult;
}

impl LspClientExt for LspClient {
    /// Schedules a query from the client.
    fn schedule_query(&self, req_id: RequestId, query_fut: QueryFuture) -> ScheduledResult {
        let fut = query_fut.map_err(|e| internal_error(e.to_string()))?;
        let fut: SchedulableResponse<CompilerQueryResponse> = Ok(match fut {
            MaybeDone::Done(res) => {
                MaybeDone::Done(res.map_err(|err| internal_error(err.to_string())))
            }
            MaybeDone::Future(fut) => MaybeDone::Future(Box::pin(async move {
                let res = fut.await;
                res.map_err(|err| internal_error(err.to_string()))
            })),
            MaybeDone::Gone => MaybeDone::Gone,
        });
        self.schedule(req_id, fut)
    }
}
