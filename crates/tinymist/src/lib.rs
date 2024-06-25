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
type ScheduledQueryResult = anyhow::Result<Option<()>>;
type ResponseFuture<T> = Pin<Box<dyn std::future::Future<Output = T> + Send>>;
type LspResponseFuture<T> = LspResult<ResponseFuture<T>>;
type QueryFuture = anyhow::Result<ResponseFuture<anyhow::Result<CompilerQueryResponse>>>;

macro_rules! just_result {
    ($expr:expr) => {
        Ok(Box::pin(ready(Ok($expr))))
    };
}
use just_result;
