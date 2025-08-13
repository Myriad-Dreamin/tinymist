#![doc = include_str!("../README.md")]

mod args;
#[cfg(feature = "export")]
mod compile;
mod generate_script;
#[cfg(feature = "preview")]
mod preview;
mod testing;
mod utils;

use core::fmt;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, LazyLock};

use clap::Parser;
use clap_builder::CommandFactory;
use clap_complete::generate;
use futures::future::MaybeDone;
use parking_lot::Mutex;
use reflexo::ImmutPath;
use reflexo_typst::package::PackageSpec;
use sync_ls::transport::{with_stdio_transport, MirrorArgs};
use sync_ls::{
    internal_error, DapBuilder, DapMessage, GetMessageKind, LsHook, LspBuilder, LspClientRoot,
    LspMessage, LspResult, Message, RequestId, TConnectionTx,
};
use tinymist::world::TaskInputs;
use tinymist::LONG_VERSION;
use tinymist::{Config, RegularInit, ServerState, SuperInit, UserActionTask};
use tinymist_project::EntryResolver;
use tinymist_query::package::PackageInfo;
use tinymist_std::hash::{FxBuildHasher, FxHashMap};
use tinymist_std::{bail, error::prelude::*};
use typst::ecow::EcoString;

#[cfg(feature = "l10n")]
use tinymist_l10n::{load_translations, set_translations};
#[cfg(feature = "preview")]
use tinymist_project::LockFile;
#[cfg(feature = "preview")]
use tinymist_task::Id;

use crate::args::*;

#[cfg(feature = "export")]
use crate::compile::compile_main;
use crate::generate_script::generate_script_main;
#[cfg(feature = "preview")]
use crate::preview::preview_main;
use crate::testing::{coverage_main, test_main};

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

static RUNTIMES: LazyLock<Runtimes> = LazyLock::new(Runtimes::default);

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
    let _ = tinymist::init_log(tinymist::InitLogOpts {
        is_transient_cmd: matches!(args.command, Some(Commands::Compile(..))),
        is_test_no_verbose: matches!(&args.command, Some(Commands::Test(test)) if !test.verbose),
        output: None,
    });

    match args.command.unwrap_or_default() {
        Commands::Completion(args) => completion(args),
        Commands::Cov(args) => coverage_main(args),
        Commands::Test(args) => RUNTIMES.tokio_runtime.block_on(test_main(args)),
        #[cfg(feature = "export")]
        Commands::Compile(args) => RUNTIMES.tokio_runtime.block_on(compile_main(args)),
        Commands::GenerateScript(args) => generate_script_main(args),
        Commands::Query(query_cmds) => query_main(query_cmds),
        Commands::Lsp(args) => lsp_main(args),
        #[cfg(feature = "dap")]
        Commands::Dap(args) => dap_main(args),
        Commands::TraceLsp(args) => trace_lsp_main(args),
        #[cfg(feature = "preview")]
        Commands::Preview(args) => RUNTIMES.tokio_runtime.block_on(preview_main(args)),
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
        let client = client_root(conn.sender);
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
#[cfg(feature = "dap")]
pub fn dap_main(args: DapArgs) -> Result<()> {
    let pairs = LONG_VERSION.trim().split('\n');
    let pairs = pairs
        .map(|e| e.splitn(2, ":").map(|e| e.trim()).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    log::info!("tinymist version information: {pairs:?}");
    log::info!("starting debug adaptor: {args:?}");

    let is_replay = !args.mirror.replay.is_empty();
    with_stdio_transport::<DapMessage>(args.mirror.clone(), |conn| {
        let client = client_root(conn.sender);
        ServerState::install_dap(DapBuilder::new(
            tinymist::DapRegularInit {
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
        let client_root = client_root(conn.sender);
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
            let w = snap.world().clone().task(TaskInputs {
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
        let client_root = client_root(conn.sender);
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
                    let path = path
                        .unwrap_or_else(|| snap.registry().resolve(&pkg).unwrap().as_ref().into());

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
                        .unwrap_or_else(|| snap.registry().resolve(&pkg).unwrap().as_ref().into());

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

#[cfg(feature = "preview")]
trait LockFileExt {
    fn preview(&mut self, doc_id: Id, args: &TaskPreviewArgs) -> Result<Id>;
}

#[cfg(feature = "preview")]
impl LockFileExt for LockFile {
    fn preview(&mut self, doc_id: Id, args: &TaskPreviewArgs) -> Result<Id> {
        use tinymist_task::{ApplyProjectTask, PreviewTask, ProjectTask, TaskWhen};

        let task_id = args
            .task_name
            .as_ref()
            .map(|t| Id::new(t.clone()))
            .unwrap_or(doc_id.clone());

        let when = args.when.clone().unwrap_or(TaskWhen::OnType);
        let task = ProjectTask::Preview(PreviewTask { when });
        let task = ApplyProjectTask {
            id: task_id.clone(),
            document: doc_id,
            task,
        };

        self.replace_task(task);

        Ok(task_id)
    }
}

/// Project document commands' main
#[cfg(feature = "lock")]
pub fn project_main(args: tinymist_project::DocCommands) -> Result<()> {
    use tinymist_project::DocCommands;

    let cwd = std::env::current_dir().context("cannot get cwd")?;
    LockFile::update(&cwd, |state| {
        let ctx: (&Path, &Path) = (&cwd, &cwd);
        match args {
            DocCommands::New(args) => {
                state.replace_document(args.to_input(ctx));
            }
            DocCommands::Configure(args) => {
                use tinymist_project::ProjectRoute;

                let id: Id = args.id.id(ctx);

                state.route.push(ProjectRoute {
                    id: id.clone(),
                    priority: args.priority,
                });
            }
        }

        Ok(())
    })
}

/// Project task commands' main
#[cfg(feature = "lock")]
pub fn task_main(args: TaskCommands) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot get cwd")?;
    LockFile::update(&cwd, |state| {
        let _ = state;
        match args {
            #[cfg(feature = "preview")]
            TaskCommands::Preview(args) => {
                let ctx: (&Path, &Path) = (&cwd, &cwd);
                let input = args.declare.to_input(ctx);
                let id = input.id.clone();
                state.replace_document(input);
                let _ = state.preview(id, &args);

                Ok(())
            }
        }
    })
}

/// Creates a new language server host.
fn client_root<M: TryFrom<Message, Error = anyhow::Error> + GetMessageKind>(
    sender: TConnectionTx<M>,
) -> LspClientRoot {
    LspClientRoot::new(RUNTIMES.tokio_runtime.handle().clone(), sender)
        .with_hook(Arc::new(TypstLsHook::default()))
}

#[derive(Default)]
struct TypstLsHook(Mutex<FxHashMap<RequestId, typst_timing::TimingScope>>);

impl fmt::Debug for TypstLsHook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TypstLsHook").finish()
    }
}

impl LsHook for TypstLsHook {
    fn start_request(&self, req_id: &RequestId, method: &str) {
        ().start_request(req_id, method);

        if let Some(scope) = typst_timing::TimingScope::new(static_str(method)) {
            let mut map = self.0.lock();
            map.insert(req_id.clone(), scope);
        }
    }

    fn stop_request(
        &self,
        req_id: &RequestId,
        method: &str,
        received_at: tinymist_std::time::Instant,
    ) {
        ().stop_request(req_id, method, received_at);

        if let Some(scope) = self.0.lock().remove(req_id) {
            let _ = scope;
        }
    }

    fn start_notification(&self, method: &str) {
        ().start_notification(method);
    }

    fn stop_notification(
        &self,
        method: &str,
        received_at: tinymist_std::time::Instant,
        result: LspResult<()>,
    ) {
        ().stop_notification(method, received_at, result);
    }
}

fn static_str(s: &str) -> &'static str {
    static STRS: Mutex<FxHashMap<EcoString, &'static str>> =
        Mutex::new(HashMap::with_hasher(FxBuildHasher));

    let mut strs = STRS.lock();
    if let Some(&s) = strs.get(s) {
        return s;
    }

    let static_ref: &'static str = String::from(s).leak();
    strs.insert(static_ref.into(), static_ref);
    static_ref
}
