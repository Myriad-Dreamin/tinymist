#![doc = include_str!("../README.md")]

mod args;

use std::{path::PathBuf, sync::Arc};

use anyhow::bail;
use clap::Parser;
use comemo::Prehashed;
use futures::future::MaybeDone;
use lsp_server::RequestId;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde_json::Value as JsonValue;
use sync_lsp::{transport::with_stdio_transport, LspBuilder, LspClient};
use tinymist::{
    CompileConfig, CompileFontOpts, Config, ConstConfig, Init, LanguageState, LspWorld, SuperInit,
};
use typst::World;
use typst::{eval::Tracer, foundations::IntoValue, syntax::Span};
use typst_ts_compiler::{CompileEnv, Compiler, TaskInputs};
use typst_ts_core::{typst::prelude::EcoVec, TypstDict};

use crate::args::{CliArguments, Commands, CompileArgs, LspArgs};

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

pub struct Runtimes {
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

static RUNTIMES: Lazy<Runtimes> = Lazy::new(Default::default);

/// The main entry point.
fn main() -> anyhow::Result<()> {
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
        Commands::Lsp(args) => lsp_main(args),
        Commands::Compile(args) => compiler_main(args),
        #[cfg(feature = "preview")]
        Commands::Preview(args) => {
            #[cfg(feature = "preview")]
            use tinymist::tool::preview::preview_main;

            RUNTIMES.tokio_runtime.block_on(preview_main(args))
        }
        Commands::Probe => Ok(()),
    }
}

pub fn lsp_main(args: LspArgs) -> anyhow::Result<()> {
    log::info!("starting LSP server: {:#?}", args);

    let is_replay = !args.mirror.replay.is_empty();

    with_stdio_transport(args.mirror.clone(), |conn| {
        let sender = Arc::new(RwLock::new(Some(conn.sender)));
        let client = LspClient::new(RUNTIMES.tokio_runtime.handle().clone(), sender);
        LanguageState::install(LspBuilder::new(
            Init {
                client: client.to_typed(),
                compile_opts: CompileFontOpts {
                    font_paths: args.font.font_paths.clone(),
                    ignore_system_fonts: args.font.ignore_system_fonts,
                    ..Default::default()
                },
                exec_cmds: Vec::new(),
            },
            client.clone(),
        ))
        .build()
        .start(conn.receiver, is_replay)
    })?;

    log::info!("LSP server did shut down");
    Ok(())
}

pub fn compiler_main(args: CompileArgs) -> anyhow::Result<()> {
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

    let inputs = Arc::new(Prehashed::new(if args.compile.inputs.is_empty() {
        TypstDict::default()
    } else {
        let pairs = args.compile.inputs.iter();
        let pairs = pairs.map(|(k, v)| (k.as_str().into(), v.as_str().into_value()));
        pairs.collect()
    }));

    with_stdio_transport(args.mirror.clone(), |conn| {
        let sender = Arc::new(RwLock::new(Some(conn.sender)));
        let client = LspClient::new(RUNTIMES.tokio_runtime.handle().clone(), sender);

        // todo: this is not elegant
        let _force_drop = client.force_drop();

        let cc = ConstConfig::default();
        let config = Config {
            compile: CompileConfig {
                roots: vec![root_path],
                font_opts: CompileFontOpts {
                    font_paths: args.compile.font.font_paths.clone(),
                    ignore_system_fonts: args.compile.font.ignore_system_fonts,
                    ..Default::default()
                },
                ..CompileConfig::default()
            },
            ..Config::default()
        };

        let mut service = LanguageState::install(LspBuilder::new(
            SuperInit {
                client: client.to_typed(),
                exec_cmds: Vec::new(),
                config,
                cc,
                err: None,
            },
            client.clone(),
        ))
        .build();

        let resp = service.ready(()).unwrap();
        let MaybeDone::Done(resp) = resp else {
            bail!("internal error: not sync init")
        };
        resp.unwrap();

        // todo: persist
        let request_received = reflexo::time::Instant::now();

        let req_id: RequestId = 0.into();
        client.register_request(
            &lsp_server::Request {
                id: req_id.clone(),
                method: "tinymistExt/documentProfiling".to_owned(),
                params: JsonValue::Null,
            },
            request_received,
        );

        let state = service.state_mut().unwrap();

        let entry = state
            .compile_config()
            .determine_entry(Some(input.as_path().into()));

        let snap = state.primary().sync_snapshot().unwrap();
        let w = snap.world.task(TaskInputs {
            entry: Some(entry),
            inputs: Some(inputs),
        });

        let mut env = CompileEnv {
            tracer: Some(Tracer::default()),
            ..Default::default()
        };
        typst_timing::enable();
        let mut errors = EcoVec::new();
        if let Err(e) = std::marker::PhantomData.compile(&w, &mut env) {
            errors = e;
        }
        let mut writer = std::io::BufWriter::new(Vec::new());
        let _ = typst_timing::export_json(&mut writer, |span| {
            resolve_span(&w, span).unwrap_or_else(|| ("unknown".to_string(), 0))
        });

        let timings = String::from_utf8(writer.into_inner().unwrap()).unwrap();

        let warnings = env.tracer.map(|e| e.warnings());

        let diagnostics = state.primary().handle.run_analysis(&w, |ctx| {
            tinymist_query::convert_diagnostics(ctx, warnings.iter().flatten().chain(errors.iter()))
        });

        let diagnostics = diagnostics.unwrap_or_default();

        client.send_notification_(lsp_server::Notification {
            method: "tinymistExt/diagnostics".to_owned(),
            params: serde_json::json!(diagnostics),
        });

        client.respond(lsp_server::Response {
            id: req_id,
            result: Some(serde_json::json!({
                "tracingData": timings,
            })),
            error: None,
        });

        Ok(())
    })?;

    Ok(())
}

/// Turns a span into a (file, line) pair.
fn resolve_span(world: &LspWorld, span: Span) -> Option<(String, u32)> {
    let id = span.id()?;
    let source = world.source(id).ok()?;
    let range = source.range(span)?;
    let line = source.byte_to_line(range.start)?;
    Some((format!("{id:?}"), line as u32 + 1))
}
