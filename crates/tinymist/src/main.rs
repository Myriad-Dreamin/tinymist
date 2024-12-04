#![doc = include_str!("../README.md")]

mod args;

use std::{
    io,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

use anyhow::bail;
use clap::Parser;
use clap_builder::CommandFactory;
use clap_complete::generate;
use futures::future::MaybeDone;
use lsp_server::RequestId;
use once_cell::sync::Lazy;
use reflexo::ImmutPath;
use reflexo_typst::{package::PackageSpec, TaskInputs, TypstDict};
use serde_json::Value as JsonValue;
use sync_lsp::{
    internal_error,
    transport::{with_stdio_transport, MirrorArgs},
    LspBuilder, LspClientRoot, LspResult,
};
use tinymist::{CompileConfig, Config, LanguageState, RegularInit, SuperInit, UserActionTask};
use tinymist_query::{package::PackageInfo, EntryResolver};
use typst::foundations::IntoValue;
use typst_shim::utils::LazyHash;

use crate::args::*;

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
            .filter_module("typlite", Info)
            .filter_module("reflexo", Info)
            .filter_module("sync_lsp", Info)
            .filter_module("reflexo_typst::service::compile", Info)
            .filter_module("reflexo_typst::service::watch", Info)
            .filter_module("reflexo_typst::diag::console", Info)
            .try_init()
    };

    // Parse command line arguments
    let args = CliArguments::parse();

    match args.command.unwrap_or_default() {
        Commands::Completion(args) => completion(args),
        Commands::Query(query_cmds) => query_main(query_cmds),
        Commands::Lsp(args) => lsp_main(args),
        Commands::TraceLsp(args) => trace_lsp_main(args),
        #[cfg(feature = "preview")]
        Commands::Preview(args) => {
            #[cfg(feature = "preview")]
            use tinymist::tool::preview::preview_main;

            RUNTIMES.tokio_runtime.block_on(preview_main(args))
        }
        Commands::Probe => Ok(()),
    }
}

/// Generates completion script to stdout.
pub fn completion(args: ShellCompletionArgs) -> anyhow::Result<()> {
    let Some(shell) = args.shell.or_else(Shell::from_env) else {
        anyhow::bail!("could not infer shell");
    };

    let mut cmd = CliArguments::command();
    generate(shell, &mut cmd, "tinymist", &mut io::stdout());

    Ok(())
}

/// The main entry point for the language server.
pub fn lsp_main(args: LspArgs) -> anyhow::Result<()> {
    let pairs = LONG_VERSION.trim().split('\n');
    let pairs = pairs
        .map(|e| e.splitn(2, ":").map(|e| e.trim()).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    log::info!("tinymist version information: {pairs:?}");
    log::info!("starting Language server: {args:#?}");

    let is_replay = !args.mirror.replay.is_empty();
    with_stdio_transport(args.mirror.clone(), |conn| {
        let client = LspClientRoot::new(RUNTIMES.tokio_runtime.handle().clone(), conn.sender);
        LanguageState::install(LspBuilder::new(
            RegularInit {
                client: client.weak().to_typed(),
                font_opts: args.font,
                exec_cmds: Vec::new(),
            },
            client.weak(),
        ))
        .build()
        .start(conn.receiver, is_replay)
    })?;

    log::info!("language server did shut down");
    Ok(())
}

/// The main entry point for the compiler.
pub fn trace_lsp_main(args: TraceLspArgs) -> anyhow::Result<()> {
    let mut input = PathBuf::from(match args.compile.input {
        Some(value) => value,
        None => return Err(anyhow::anyhow!("provide a valid path")),
    });

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

    let inputs = Arc::new(LazyHash::new(if args.compile.inputs.is_empty() {
        TypstDict::default()
    } else {
        let pairs = args.compile.inputs.iter();
        let pairs = pairs.map(|(k, v)| (k.as_str().into(), v.as_str().into_value()));
        pairs.collect()
    }));

    with_stdio_transport(args.mirror.clone(), |conn| {
        let client_root = LspClientRoot::new(RUNTIMES.tokio_runtime.handle().clone(), conn.sender);
        let client = client_root.weak();
        let roots = vec![ImmutPath::from(root_path)];
        let config = Config {
            compile: CompileConfig {
                entry_resolver: EntryResolver {
                    roots,
                    ..Default::default()
                },
                font_opts: args.compile.font,
                ..CompileConfig::default()
            },
            ..Config::default()
        };

        let mut service = LanguageState::install(LspBuilder::new(
            SuperInit {
                client: client.to_typed(),
                exec_cmds: Vec::new(),
                config,
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

        let entry = state.entry_resolver().resolve(Some(input.as_path().into()));

        let snap = state.primary().snapshot().unwrap();

        RUNTIMES.tokio_runtime.block_on(async {
            let snap = snap.receive().await.unwrap();

            let w = snap.world.task(TaskInputs {
                entry: Some(entry),
                inputs: Some(inputs),
            });

            UserActionTask::trace_main(client, state, &w, args.rpc_kind, req_id).await
        });

        Ok(())
    })?;

    Ok(())
}

/// The main entry point for language server queries.
pub fn query_main(cmds: QueryCommands) -> anyhow::Result<()> {
    use reflexo_typst::package::PackageRegistry;

    with_stdio_transport(MirrorArgs::default(), |conn| {
        let client_root = LspClientRoot::new(RUNTIMES.tokio_runtime.handle().clone(), conn.sender);
        let client = client_root.weak();

        // todo: roots, inputs, font_opts
        let config = Config::default();

        let mut service = LanguageState::install(LspBuilder::new(
            SuperInit {
                client: client.to_typed(),
                exec_cmds: Vec::new(),
                config,
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

        let state = service.state_mut().unwrap();

        let snap = state.primary().snapshot().unwrap();
        let res = RUNTIMES.tokio_runtime.block_on(async move {
            let w = snap.receive().await.map_err(internal_error)?;
            match cmds {
                QueryCommands::PackageDocs(args) => {
                    let pkg = PackageSpec::from_str(&args.id).unwrap();
                    let path = args.path.map(PathBuf::from);
                    let path = path
                        .unwrap_or_else(|| w.world.registry.resolve(&pkg).unwrap().as_ref().into());

                    let res = state
                        .resource_package_docs_(PackageInfo {
                            path,
                            namespace: pkg.namespace,
                            name: pkg.name,
                            version: pkg.version.to_string(),
                        })?
                        .await?;

                    let output_path = Path::new(&args.output);
                    std::fs::write(output_path, res).map_err(internal_error)?;
                }
                QueryCommands::CheckPackage(args) => {
                    let pkg = PackageSpec::from_str(&args.id).unwrap();
                    let path = args.path.map(PathBuf::from);
                    let path = path
                        .unwrap_or_else(|| w.world.registry.resolve(&pkg).unwrap().as_ref().into());

                    state
                        .check_package(PackageInfo {
                            path,
                            namespace: pkg.namespace,
                            name: pkg.name,
                            version: pkg.version.to_string(),
                        })?
                        .await?;
                }
            };

            LspResult::Ok(())
        });

        res.map_err(|e| anyhow::anyhow!("{e:?}"))
    })?;

    Ok(())
}
