#![doc = include_str!("../README.md")]

mod args;

use args::CompileArgs;
use clap::Parser;
use lsp_server::{ErrorCode, ResponseError};
use lsp_types::InitializeParams;
use tinymist::{
    harness::{lsp_harness, InitializedLspDriver, LspDriver, LspHost},
    init::Init,
    transport::with_stdio_transport,
    CompileFontOpts, CompileOpts, TypstLanguageServer,
};

use crate::args::{CliArguments, Commands, LspArgs};

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

    match args.command.unwrap_or_default() {
        Commands::Lsp(args) => lsp_main(args),
        Commands::Compile(args) => compiler_main(args),
        Commands::Probe => Ok(()),
    }
}

pub fn lsp_main(args: LspArgs) -> anyhow::Result<()> {
    log::info!("starting generic LSP server");

    with_stdio_transport(args.mirror.clone(), |conn, force_exit| {
        lsp_harness(Lsp { args }, conn, force_exit)
    })?;

    log::info!("LSP server did shut down");

    struct Lsp {
        args: LspArgs,
    }

    impl LspDriver for Lsp {
        type InitParams = InitializeParams;
        type InitResult = lsp_types::InitializeResult;
        type InitializedSelf = TypstLanguageServer;

        fn initialize(
            self,
            host: LspHost<Self::InitializedSelf>,
            params: Self::InitParams,
        ) -> (
            Self::InitializedSelf,
            Result<Self::InitResult, lsp_server::ResponseError>,
        ) {
            Init {
                host,
                compile_opts: CompileOpts {
                    font: CompileFontOpts {
                        font_paths: self.args.font.font_paths.clone(),
                        no_system_fonts: self.args.font.no_system_fonts,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            }
            .initialize(params)
        }
    }

    Ok(())
}

pub fn compiler_main(args: CompileArgs) -> anyhow::Result<()> {
    log::info!("starting compile server");

    with_stdio_transport(args.mirror.clone(), |conn, force_exit| {
        lsp_harness(Lsp {}, conn, force_exit)
    })?;

    log::info!("compile server did shut down");

    struct Lsp {}

    impl LspDriver for Lsp {
        type InitParams = InitializeParams;
        type InitResult = lsp_types::InitializeResult;
        type InitializedSelf = CompileServer;

        fn initialize(
            self,
            _host: LspHost<Self::InitializedSelf>,
            _params: Self::InitParams,
        ) -> (
            Self::InitializedSelf,
            Result<Self::InitResult, lsp_server::ResponseError>,
        ) {
            (CompileServer, Err(internal_error("not implemented")))
        }
    }

    struct CompileServer;

    impl InitializedLspDriver for CompileServer {
        fn initialized(&mut self, _params: lsp_types::InitializedParams) {
            todo!()
        }

        fn main_loop(
            &mut self,
            _receiver: crossbeam_channel::Receiver<lsp_server::Message>,
        ) -> anyhow::Result<()> {
            todo!()
        }
    }

    Ok(())
}

fn internal_error(msg: impl Into<String>) -> ResponseError {
    ResponseError {
        code: ErrorCode::InternalError as i32,
        message: msg.into(),
        data: None,
    }
}
