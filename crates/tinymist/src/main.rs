#![doc = include_str!("../README.md")]

mod args;

use std::io::Write;

use clap::Parser;
use log::info;
use tinymist::{logging::LogService, TypstLanguageServer};
use tokio::io::AsyncRead;
use tokio_util::io::InspectReader;
use tower_lsp::{LspService, Server};

use crate::args::CliArguments;

/// The main entry point.
#[tokio::main]
async fn main() {
    // Start logging
    let _ = {
        use log::LevelFilter::*;
        env_logger::builder()
            .filter_module("tinymist", Info)
            .filter_module("typst_preview", Debug)
            .filter_module("typst_ts", Info)
            .filter_module("typst_ts_compiler::service::compile", Info)
            .filter_module("typst_ts_compiler::service::watch", Debug)
            .try_init()
    };

    // Parse command line arguments
    let args = CliArguments::parse();
    info!("Arguments: {:#?}", args);

    // Set up input and output
    let stdin: Box<dyn AsyncRead + Unpin> = if !args.replay.is_empty() {
        // Get input from file
        let file = tokio::fs::File::open(&args.replay).await.unwrap();
        Box::new(file)
    } else if args.mirror.is_empty() {
        // Get input from stdin
        Box::new(tokio::io::stdin())
    } else {
        // Get input from stdin and mirror to file
        let mut file = std::fs::File::create(&args.mirror).unwrap();
        Box::new(InspectReader::new(tokio::io::stdin(), move |bytes| {
            file.write_all(bytes).unwrap();
        }))
    };
    let stdout = tokio::io::stdout();

    // Set up LSP server
    let (inner, socket) = LspService::new(TypstLanguageServer::new);
    let service = LogService {
        inner,
        show_time: true,
    };

    // Handle requests
    Server::new(stdin, stdout, socket).serve(service).await;
}
