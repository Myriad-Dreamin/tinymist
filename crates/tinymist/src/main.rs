#![doc = include_str!("../README.md")]

mod args;

use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use clap::Parser;
use clap_builder::CommandFactory;
use clap_complete::generate;
use futures::future::MaybeDone;
use once_cell::sync::Lazy;
use reflexo::ImmutPath;
use reflexo_typst::package::PackageSpec;
use sync_ls::transport::{with_stdio_transport, MirrorArgs};
use sync_ls::{
    internal_error, DapBuilder, DapMessage, LspBuilder, LspClientRoot, LspMessage, LspResult,
    RequestId,
};
use tinymist::tool::project::{compile_main, generate_script_main, project_main, task_main};
use tinymist::tool::testing::{coverage_main, test_main};
use tinymist::world::TaskInputs;
use tinymist::{Config, DapRegularInit, RegularInit, ServerState, SuperInit, UserActionTask};
use tinymist_core::LONG_VERSION;
use tinymist_project::EntryResolver;
use tinymist_query::package::PackageInfo;
use tinymist_std::{bail, error::prelude::*};

#[cfg(feature = "l10n")]
use tinymist_l10n::{load_translations, set_translations};

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

static RUNTIMES: Lazy<Runtimes> = Lazy::new(Runtimes::default);

/// The main entry point.
fn main() -> Result<()> {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    // Parses command line arguments
    let args = CliArguments::parse();

    // Probes soon to avoid other initializations causing errors
    if matches!(args.command, Some(Commands::Probe)) {
        return Ok(());
    }

    // Loads translations
    #[cfg(feature = "l10n")]
    set_translations(load_translations(tinymist_assets::L10N_DATA)?);

    // Starts logging
    let _ = {
        let is_transient_cmd = matches!(args.command, Some(Commands::Compile(..)));
        let is_test_no_verbose =
            matches!(&args.command, Some(Commands::Test(test)) if !test.verbose);
        use log::LevelFilter::*;
        let base_no_info = is_transient_cmd || is_test_no_verbose;
        let base_level = if base_no_info { Warn } else { Info };
        let preview_level = if is_test_no_verbose { Warn } else { Debug };
        let diag_level = if is_test_no_verbose { Warn } else { Info };

        env_logger::builder()
            .filter_module("tinymist", base_level)
            .filter_module("typst_preview", preview_level)
            .filter_module("typlite", base_level)
            .filter_module("reflexo", base_level)
            .filter_module("sync_ls", base_level)
            .filter_module("reflexo_typst2vec::pass::span2vec", Error)
            .filter_module("reflexo_typst::diag::console", diag_level)
            .try_init()
    };

    match args.command.unwrap_or_default() {
        Commands::Completion(args) => completion(args),
        Commands::Cov(args) => coverage_main(args),
        Commands::Test(args) => RUNTIMES.tokio_runtime.block_on(test_main(args)),
        Commands::Compile(args) => RUNTIMES.tokio_runtime.block_on(compile_main(args)),
        Commands::GenerateScript(args) => generate_script_main(args),
        Commands::Query(query_cmds) => query_main(query_cmds),
        Commands::Lsp(args) => lsp_main(args),
        Commands::Dap(args) => dap_main(args),
        Commands::TraceLsp(args) => trace_lsp_main(args),
        #[cfg(feature = "preview")]
        Commands::Preview(args) => {
            #[cfg(feature = "preview")]
            use tinymist::tool::preview::preview_main;

            RUNTIMES.tokio_runtime.block_on(preview_main(args))
        }
        Commands::Doc(args) => project_main(args),
        Commands::Task(args) => task_main(args),
        Commands::Probe => Ok(()),
    }
}

/// Generates completion script to stdout.
pub fn completion(args: ShellCompletionArgs) -> Result<()> {
    let Some(shell) = args.shell.or_else(Shell::from_env) else {
        tinymist_std::bail!("could not infer shell");
    };

    let mut cmd = CliArguments::command();
    generate(shell, &mut cmd, "tinymist", &mut io::stdout());

    Ok(())
}

/// The main entry point for the language server.
pub fn lsp_main(args: LspArgs) -> Result<()> {
    let pairs = LONG_VERSION.trim().split('\n');
    let pairs = pairs
        .map(|e| e.splitn(2, ":").map(|e| e.trim()).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    log::info!("tinymist version information: {pairs:?}");
    log::info!("starting language server: {args:?}");

    let is_replay = !args.mirror.replay.is_empty();
    with_stdio_transport::<LspMessage>(args.mirror.clone(), |conn| {
        let client = LspClientRoot::new(RUNTIMES.tokio_runtime.handle().clone(), conn.sender);
        ServerState::install_lsp(LspBuilder::new(
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

/// The main entry point for the language server.
pub fn dap_main(args: DapArgs) -> Result<()> {
    let pairs = LONG_VERSION.trim().split('\n');
    let pairs = pairs
        .map(|e| e.splitn(2, ":").map(|e| e.trim()).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    log::info!("tinymist version information: {pairs:?}");
    log::info!("starting debug adaptor: {args:?}");

    let is_replay = !args.mirror.replay.is_empty();
    with_stdio_transport::<DapMessage>(args.mirror.clone(), |conn| {
        let client = LspClientRoot::new(RUNTIMES.tokio_runtime.handle().clone(), conn.sender);
        ServerState::install_dap(DapBuilder::new(
            DapRegularInit {
                client: client.weak().to_typed(),
                font_opts: args.font,
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
pub fn trace_lsp_main(args: TraceLspArgs) -> Result<()> {
    let inputs = args.compile.resolve_inputs();
    let mut input = PathBuf::from(match args.compile.input {
        Some(value) => value,
        None => Err(anyhow::anyhow!("provide a valid path"))?,
    });
    let mut root_path = args.compile.root.unwrap_or(PathBuf::from("."));

    if root_path.is_relative() {
        root_path = std::env::current_dir().context("cwd")?.join(root_path);
    }
    if input.is_relative() {
        input = std::env::current_dir().context("cwd")?.join(input);
    }
    if !input.starts_with(&root_path) {
        bail!("input file is not within the root path: {input:?} not in {root_path:?}");
    }

    with_stdio_transport::<LspMessage>(args.mirror.clone(), |conn| {
        let client_root = LspClientRoot::new(RUNTIMES.tokio_runtime.handle().clone(), conn.sender);
        let client = client_root.weak();
        let roots = vec![ImmutPath::from(root_path)];
        let config = Config {
            entry_resolver: EntryResolver {
                roots,
                ..EntryResolver::default()
            },
            font_opts: args.compile.font,
            ..Config::default()
        };

        let mut service = ServerState::install_lsp(LspBuilder::new(
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
            anyhow::bail!("internal error: not sync init")
        };
        resp.unwrap();

        // todo: persist
        let request_received = reflexo::time::Instant::now();

        let req_id: RequestId = 0.into();
        client.register_request("tinymistExt/documentProfiling", &req_id, request_received);

        let state = service.state_mut().unwrap();

        let entry = state.entry_resolver().resolve(Some(input.as_path().into()));

        let snap = state.snapshot().unwrap();

        RUNTIMES.tokio_runtime.block_on(async {
            let w = snap.world.task(TaskInputs {
                entry: Some(entry),
                inputs,
            });

            UserActionTask::trace_main(client, state, &w, args.rpc_kind, req_id).await
        });

        Ok(())
    })?;

    Ok(())
}

/// The main entry point for language server queries.
pub fn query_main(cmds: QueryCommands) -> Result<()> {
    use tinymist_project::package::PackageRegistry;

    with_stdio_transport::<LspMessage>(MirrorArgs::default(), |conn| {
        let client_root = LspClientRoot::new(RUNTIMES.tokio_runtime.handle().clone(), conn.sender);
        let client = client_root.weak();

        // todo: roots, inputs, font_opts
        let config = Config::default();

        let mut service = ServerState::install_lsp(LspBuilder::new(
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
            anyhow::bail!("internal error: not sync init")
        };
        resp.unwrap();

        let state = service.state_mut().unwrap();

        let snap = state.snapshot().unwrap();
        let res = RUNTIMES.tokio_runtime.block_on(async move {
            match cmds {
                QueryCommands::PackageDocs(args) => {
                    let pkg = PackageSpec::from_str(&args.id).unwrap();
                    let path = args.path.map(PathBuf::from);
                    let path = path.unwrap_or_else(|| {
                        snap.world.registry.resolve(&pkg).unwrap().as_ref().into()
                    });

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
                    let path = path.unwrap_or_else(|| {
                        snap.world.registry.resolve(&pkg).unwrap().as_ref().into()
                    });

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
