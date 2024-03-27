#![doc = include_str!("../README.md")]

mod args;

use std::{path::PathBuf, sync::Arc};

use args::CompileArgs;
use clap::Parser;
use comemo::Prehashed;
use lsp_types::{InitializeParams, InitializedParams};
use parking_lot::RwLock;
use tinymist::{
    compiler_init::{CompileInit, CompileInitializeParams},
    harness::{lsp_harness, InitializedLspDriver, LspDriver, LspHost},
    transport::with_stdio_transport,
    CompileFontOpts, CompileOpts, Init, TypstLanguageServer,
};
use tokio::sync::mpsc;
use typst::foundations::IntoValue;
use typst_ts_compiler::service::{CompileReport, Compiler, ConsoleDiagReporter};
use typst_ts_core::GenericExporter;
use typst_ts_core::TypstDict;

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

    match args.command.unwrap_or_default() {
        Commands::Lsp(args) => lsp_main(args),
        Commands::Compile(args) => compiler_main(args).await,
        Commands::Probe => Ok(()),
    }
}

pub fn lsp_main(args: LspArgs) -> anyhow::Result<()> {
    log::info!("starting generic LSP server: {:#?}", args);

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

pub async fn compiler_main(args: CompileArgs) -> anyhow::Result<()> {
    let (diag_tx, _) = mpsc::unbounded_channel();

    let mut input = PathBuf::from(args.compile.input.unwrap());

    let mut root_path = args.compile.root.unwrap_or(PathBuf::from("."));

    if root_path.is_relative() {
        root_path = std::env::current_dir()?.join(root_path);
    }
    if input.is_relative() {
        input = std::env::current_dir()?.join(input);
    }
    if !input.starts_with(&root_path) {
        return Err(anyhow::anyhow!(
            "input file is not within the root path: {input:?} not in {root_path:?}"
        ));
    }

    let inputs = Arc::new(Prehashed::new(if args.compile.inputs.is_empty() {
        TypstDict::default()
    } else {
        let pairs = args.compile.inputs.iter();
        let pairs = pairs.map(|(k, v)| (k.as_str().into(), v.as_str().into_value()));
        pairs.collect()
    }));

    let init = CompileInit {
        handle: tokio::runtime::Handle::current(),
        font: CompileFontOpts {
            font_paths: args.compile.font.font_paths.clone(),
            no_system_fonts: args.compile.font.no_system_fonts,
            ..Default::default()
        },
        diag_tx,
    };
    if args.persist {
        log::info!("starting compile server");

        with_stdio_transport(args.mirror.clone(), |conn, force_exit| {
            lsp_harness(init, conn, force_exit)
        })?;

        log::info!("compile server did shut down");
    } else {
        {
            let sender = Arc::new(RwLock::new(None));
            let host = LspHost::new(sender.clone());

            let _drop_connection = ForceDrop(sender);

            let (mut service, res) = init.initialize(
                host,
                CompileInitializeParams {
                    config: serde_json::json!({
                        "rootPath": root_path,
                    }),
                    position_encoding: None,
                },
            );

            res.unwrap();

            service.initialized(InitializedParams {});

            service
                .compiler()
                .change_entry(Some(input.as_path().into()))
                .unwrap();
            let doc = service
                .compiler()
                .steal_async(|w, _| {
                    w.compiler.world_mut().inputs = inputs;

                    let mut env = Default::default();
                    let t0 = std::time::Instant::now();
                    match w.compiler.pure_compile(&mut env) {
                        Ok(doc) => Some(doc),
                        Err(e) => {
                            let reporter = ConsoleDiagReporter::default();

                            let res = reporter.export(
                                w.compiler.world(),
                                Arc::new((
                                    env.features.clone(),
                                    CompileReport::CompileError(
                                        w.compiler.main_id(),
                                        e,
                                        t0.elapsed(),
                                    ),
                                )),
                            );
                            if let Err(e) = res {
                                log::error!("failed to report diagnostics: {e:?}");
                            }

                            None
                        }
                    }
                })
                .await
                .unwrap();

            let Some(doc) = doc else {
                return Err(anyhow::anyhow!("cannot compile"));
            };

            let p = typst_pdf::pdf(&doc, typst::foundations::Smart::Auto, None);
            let output = input.with_extension("pdf");
            tokio::fs::write(output, p).await.unwrap();
        }
    }

    Ok(())
}

struct ForceDrop<T>(Arc<RwLock<Option<T>>>);
impl<T> Drop for ForceDrop<T> {
    fn drop(&mut self) {
        self.0.write().take();
    }
}
