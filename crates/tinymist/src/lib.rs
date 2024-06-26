//! # tinymist
//!
//! This crate provides an integrated service for [Typst](https://typst.app/). It provides:
//! + A language server following the [Language Server Protocol](https://microsoft.github.io/language-server-protocol/).
//!
//! ## Architecture
//!
//! Tinymist binary has multiple modes, and it may runs multiple actors in
//! background. The actors could run as an async task, in a single thread, or in
//! an isolated process.
//!
//! The main process of tinymist runs the program as a language server, through
//! stdin and stdout. A main process will fork:
//! - rendering actors to provide PDF export with watching.
//! - compiler actors to provide language APIs.
//!
//! ## Debugging with input mirroring
//!
//! You can record the input during running the editors with Tinymist. You can
//! then replay the input to debug the language server.
//!
//! ```sh
//! # Record the input
//! tinymist lsp --mirror input.txt
//! # Replay the input
//! tinymist lsp --replay input.txt
//! ```

// pub mod formatting;
mod actor;
pub mod harness;
mod resource;
mod server;
mod state;
pub mod tools;
pub mod transport;
mod utils;
mod world;
use std::pin::Pin;

pub use crate::harness::LspHost;
use futures::future::MaybeDone;
use serde_json::Value as JsonValue;
pub use server::compile;
pub use server::compile_init;
pub use server::lsp::*;
pub use server::lsp_init::*;
#[cfg(feature = "preview")]
pub use server::preview;
use tinymist_query::CompilerQueryResponse;
pub use world::{
    CompileFontOpts, CompileOnceOpts, CompileOpts, LspUniverse, LspWorld, LspWorldBuilder,
};

// use async_lsp::ClientSocket;
use lsp_server::ResponseError;

type LspResult<Res> = Result<Res, ResponseError>;

type ScheduledResult = LspResult<Option<()>>;
type ResponseFuture<T> = MaybeDone<Pin<Box<dyn std::future::Future<Output = T> + Send>>>;
type LspResponseFuture<T> = LspResult<ResponseFuture<T>>;
type QueryFuture = anyhow::Result<ResponseFuture<anyhow::Result<CompilerQueryResponse>>>;
type SchedulableResponse<T> = LspResponseFuture<LspResult<T>>;
type AnySchedulableResponse = SchedulableResponse<JsonValue>;

macro_rules! just_ok {
    ($expr:expr) => {
        Ok(futures::future::MaybeDone::Done(Ok($expr)))
    };
}
use just_ok;

macro_rules! just_result {
    ($expr:expr) => {
        Ok(futures::future::MaybeDone::Done($expr))
    };
}
use just_result;

#[allow(unused)]
macro_rules! just_future {
    ($expr:expr) => {
        Ok(futures::future::MaybeDone::Future(Box::pin($expr)))
    };
}
#[allow(unused_imports)]
use just_future;
