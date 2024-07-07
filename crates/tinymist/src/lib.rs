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
mod resource;
mod server;
mod state;
pub mod tool;
mod utils;
mod world;

use lsp_server::ErrorCode;
use lsp_server::ResponseError;
pub use server::compile;
pub use server::compile_init;
pub use server::lsp::*;
pub use server::lsp_init::*;
#[cfg(feature = "preview")]
pub use server::preview;
pub use world::{
    CompileFontOpts, CompileOnceOpts, CompileOpts, LspUniverse, LspWorld, LspWorldBuilder,
};

pub use sync_lsp::LspClient;
use sync_lsp::*;

/// Get a parsed command argument.
/// Return `INVALID_PARAMS` when no arg or parse failed.
macro_rules! get_arg {
    ($args:ident[$idx:expr] as $ty:ty) => {{
        let arg = $args.get_mut($idx);
        let arg = arg.and_then(|x| from_value::<$ty>(x.take()).ok());
        match arg {
            Some(v) => v,
            None => {
                let msg = concat!("expect ", stringify!($ty), "at args[", $idx, "]");
                return Err(invalid_params(msg));
            }
        }
    }};
}
use get_arg;

/// Get a parsed command argument or default if no arg.
/// Return `INVALID_PARAMS` when parse failed.
macro_rules! get_arg_or_default {
    ($args:ident[$idx:expr] as $ty:ty) => {{
        if $idx >= $args.len() {
            Default::default()
        } else {
            get_arg!($args[$idx] as $ty)
        }
    }};
}
use get_arg_or_default;

pub fn z_internal_error(msg: typst_ts_core::Error) -> ResponseError {
    ResponseError {
        code: ErrorCode::InternalError as i32,
        message: format!("internal: {msg:?}"),
        data: None,
    }
}
