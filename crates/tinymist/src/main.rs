#![doc = include_str!("../README.md")]

mod args;
mod modes;

use clap::Parser;

use crate::args::CliArguments;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// The main entry point.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    // Start logging
    let _ = {
        use log::LevelFilter::*;
        env_logger::builder()
            .filter_module("tinymist", Info)
            .filter_module("typst_preview", Debug)
            .filter_module("typst_ts", Info)
            .filter_module("typst_ts_compiler::service::compile", Info)
            .filter_module("typst_ts_compiler::service::watch", Info)
            .try_init()
    };

    // Parse command line arguments
    let args = CliArguments::parse();
    log::info!("Arguments: {:#?}", args);

    match args.mode.as_str() {
        "server" => modes::lsp_main(args),
        "probe" => Ok(()),
        _ => Err(anyhow::anyhow!(
            "unknown mode: {mode}, expected one of: server or probe",
            mode = args.mode,
        )),
    }
}
