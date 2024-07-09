#![doc = include_str!("../README.md")]

mod args;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::bail;
use async_lsp::LspService;
use clap::Parser;
use lsp_server::RequestId;
use lsp_types::request::Request;
use serde_json::Value as JsonValue;
use sync_lsp::lifecycle::Initializer;
use sync_lsp::transport::with_memory_transport;
use sync_lsp::{transport::with_stdio_transport, LspClient};
use tinymist::{CompileConfig, Config, ConstConfig, RegularInit, SuperInit};

use crate::args::{CliArguments, Commands, CompileArgs, LspArgs};

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// The runtimes used by the application.
pub struct Runtimes {
    /// The tokio runtime.
    pub tokio_runtime: tokio::runtime::Runtime,
}

impl Default for Runtimes {
    fn default() -> Self {
        let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        Self { tokio_runtime }
    }
}

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
            .filter_module("sync_lsp", Info)
            .filter_module("typst_ts_compiler::service::compile", Info)
            .filter_module("typst_ts_compiler::service::watch", Info)
            .try_init()
    };

    // Parse command line arguments
    let args = CliArguments::parse();

    match args.command.unwrap_or_default() {
        Commands::Lsp(args) => lsp_main(args).await,
        Commands::Compile(args) => compiler_main(args).await,
        #[cfg(feature = "preview")]
        Commands::Preview(args) => {
            #[cfg(feature = "preview")]
            use tinymist::tool::preview::preview_main;

            preview_main(args).await
        }
        Commands::Probe => Ok(()),
    }
}

fn lsp_harness<D: Initializer>(
    driver: D,
) -> impl LspService<Response = JsonValue, Error = async_lsp::ResponseError> {
    // todo: the follow code is gone.
    // // Start the LSP server
    // let mut force_exit = false;

    // f(connection, &mut force_exit)?;

    // if !force_exit {
    //     io_threads.join()?;
    // }
    tower::ServiceBuilder::new()
        .layer(sync_lsp::lifecycle::StagedLifecycleLayer::default())
        // .layer(LifecycleLayer::default())
        // TODO: Use `CatchUnwindLayer`.
        // .layer(ConcurrencyLayer::new(concurrency))
        // .layer(ClientProcessMonitorLayer::new(client.clone()))
        .service(driver)
}

/// The main entry point for the LSP server.
pub async fn lsp_main(args: LspArgs) -> anyhow::Result<()> {
    log::info!("starting LSP server: {:#?}", args);

    let handle = tokio::runtime::Handle::current();
    with_stdio_transport(args.mirror.clone(), |conn| {
        let client = LspClient::new(handle, conn);
        lsp_harness(RegularInit {
            client,
            font_opts: args.font,
        })
    })
    .await?;

    log::info!("LSP server did shut down");
    Ok(())
}

/// The main entry point for the compiler.
pub async fn compiler_main(args: CompileArgs) -> anyhow::Result<()> {
    let mut input = PathBuf::from(args.compile.input.unwrap());

    let mut root_path = args.compile.root.unwrap_or(PathBuf::from("."));

    if root_path.is_relative() {
        root_path = std::env::current_dir()?.join(root_path);
    }
    if input.is_relative() {
        input = std::env::current_dir()?.join(input);
    }
    if !input.starts_with(&root_path) {
        bail!("input file is not within the root path: {input:?} not in {root_path:?}");
    }

    let inputs = args.compile.inputs;

    let handle = tokio::runtime::Handle::current();
    with_memory_transport(args.mirror.clone(), |w, conn| {
        let client = LspClient::new(handle.clone(), conn);
        let cc = ConstConfig::default();
        let config = Config {
            compile: CompileConfig {
                roots: vec![root_path],
                font_opts: args.compile.font,
                ..CompileConfig::default()
            },
            ..Config::default()
        };

        // todo: persist
        handle.spawn_blocking(move || {
            let mut w = tokio_util::io::SyncIoBridge::new(w);

            use lsp_types::notification::Exit;
            use lsp_types::notification::Initialized;
            use lsp_types::notification::Notification;
            use lsp_types::request::Initialize;
            use lsp_types::request::Shutdown;
            use lsp_types::InitializedParams;

            let req_id: RequestId = 0.into();
            lsp_server::Message::write(
                lsp_server::Message::Request(lsp_server::Request {
                    id: req_id.clone(),
                    method: Initialize::METHOD.to_owned(),
                    params: serde_json::json!(()),
                }),
                &mut w,
            )
            .unwrap();

            lsp_server::Message::write(
                lsp_server::Message::Notification(lsp_server::Notification {
                    method: Initialized::METHOD.to_owned(),
                    params: serde_json::json!(InitializedParams {}),
                }),
                &mut w,
            )
            .unwrap();

            let req_id: RequestId = 1.into();
            lsp_server::Message::write(
                lsp_server::Message::Request(lsp_server::Request {
                    id: req_id.clone(),
                    method: "tinymistExt/documentProfiling".to_owned(),
                    params: serde_json::json!((input, inputs)),
                }),
                &mut w,
            )
            .unwrap();

            std::thread::sleep(Duration::from_secs(10));

            let req_id: RequestId = 2.into();
            lsp_server::Message::write(
                lsp_server::Message::Request(lsp_server::Request {
                    id: req_id.clone(),
                    method: Shutdown::METHOD.to_owned(),
                    params: serde_json::json!(()),
                }),
                &mut w,
            )
            .unwrap();

            lsp_server::Message::write(
                lsp_server::Message::Notification(lsp_server::Notification {
                    method: Exit::METHOD.to_owned(),
                    params: serde_json::json!(()),
                }),
                &mut w,
            )
            .unwrap();
        });

        lsp_harness(SuperInit {
            client: client.clone(),
            config,
            cc,
            err: None,
        })
    })
    .await?;

    // Ok(mainloop.run_buffered(i, o).await?)

    // let client_root = LspClient::new(RUNTIMES.tokio_runtime.handle().clone(),
    // conn.sender); let client = client_root.weak();

    // let mut service = LanguageState::install(LspBuilder::new(
    //     ,
    //     client.clone(),
    // ))
    // .build();

    Ok(())
}
