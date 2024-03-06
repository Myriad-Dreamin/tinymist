//! # tinymist LSP Server

mod config;
mod ext;
mod lsp_typst_boundary;

// pub mod formatting;
pub mod actor;
pub mod analysis;
pub mod lsp;
pub mod semantic_tokens;

use tower_lsp::{LspService, Server};

use lsp::TypstServer;

// #[derive(Debug, Clone)]
// struct Args {}

// fn arg_parser() -> OptionParser<Args> {
//     construct!(Args {}).to_options().version(
//         format!(
//             "{}, commit {} (Typst version {TYPST_VERSION})",
//             env!("CARGO_PKG_VERSION"),
//             env!("GIT_COMMIT")
//         )
//         .as_str(),
//     )
// }

// pub const TYPST_VERSION: &str = env!("TYPST_VERSION");

#[tokio::main]
async fn main() {
    let _ = env_logger::builder()
        // TODO: set this back to Info
        .filter_module("tinymist", log::LevelFilter::Trace)
        // .filter_module("tinymist", log::LevelFilter::Debug)
        .filter_module("typst_preview", log::LevelFilter::Debug)
        .filter_module("typst_ts", log::LevelFilter::Info)
        // TODO: set this back to Info
        .filter_module(
            "typst_ts_compiler::service::compile",
            log::LevelFilter::Debug,
        )
        .filter_module("typst_ts_compiler::service::watch", log::LevelFilter::Debug)
        .try_init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(TypstServer::new);

    Server::new(stdin, stdout, socket).serve(service).await;
}
