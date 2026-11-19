//! Project management tools.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use tinymist::project::*;
use tinymist::tool::project::{ProjectOpts, start_project};
use tinymist::world::WorldComputeGraph;
use tinymist::world::system::print_diagnostics;
use tinymist::{Config, ExportTask};
use tinymist_query::OnExportResponse;
use tinymist_query::analysis::Analysis;
use tinymist_std::{ImmutPath, error::prelude::*};
use tokio::sync::watch;

/// Specify the project compilation.
#[derive(Debug, Clone, clap::Parser)]
pub struct CompileArgs {
    /// Specify the compile task related arguments.
    #[clap(flatten)]
    pub compile: TaskCompileArgs,

    /// Save the compilation arguments to the lock file. If `--lockfile` is not
    /// set, the lock file will be saved in the cwd.
    #[clap(long)]
    pub save_lock: bool,

    /// Specify the path to the lock file. If the path is set, the lockfile will
    /// be saved (--save-lock).
    #[clap(long)]
    pub lockfile: Option<PathBuf>,

    /// Watch the input files and export again on every change, like `typst
    /// watch`. Prints `compiled successfully` after each export.
    #[clap(long)]
    pub watch: bool,

    /// Run a command after every successful export in watch mode, with the
    /// path of the written file as its last argument and in the environment
    /// variable `TINYMIST_OUTPUT`. The command runs through the system shell;
    /// a failure is reported but does not stop watching.
    #[clap(long, value_name = "COMMAND", requires = "watch")]
    pub on_export: Option<String>,
}

/// Runs project compilation(s)
pub async fn compile_main(args: CompileArgs) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot get cwd")?;
    // todo: respect the name of the lock file

    // Saves the lock file if the flags are set
    let save_lock = args.save_lock || args.lockfile.is_some();

    let lock_dir: ImmutPath = if let Some(lockfile) = args.lockfile {
        let lockfile = if lockfile.is_absolute() {
            lockfile
        } else {
            cwd.join(lockfile)
        };
        lockfile
            .parent()
            .context("lock file must have a parent directory")?
            .into()
    } else {
        cwd.as_path().into()
    };

    // Identifies the input and output
    let input = args.compile.declare.to_input((&cwd, &lock_dir));
    let output = args.compile.to_task(input.id.clone(), &cwd)?;

    if save_lock {
        LockFile::update(&lock_dir, |state| {
            state.replace_document(input.relative_to(&lock_dir));
            state.replace_task(output.clone());

            Ok(())
        })?;
    }

    // Prepares for the compilation
    let universe = (input, lock_dir.clone()).resolve()?;
    let lock_dir = save_lock.then_some(lock_dir);

    if args.watch {
        return watch_main(universe, output.task, lock_dir, args.on_export).await;
    }

    let world = universe.snapshot();
    let graph = WorldComputeGraph::from_world(world);

    // Compiles the project
    let compiled = if matches!(output.task, ProjectTask::ExportBundle(..)) {
        CompiledArtifact::from_graph_without_doc(graph)
    } else {
        let is_html = matches!(output.task, ProjectTask::ExportHtml(..));
        CompiledArtifact::from_graph(graph, is_html)
    };

    let diag = compiled.diagnostics();
    print_diagnostics(compiled.world(), diag, DiagnosticFormat::Human)
        .context_ut("print diagnostics")?;

    if compiled.has_errors() {
        // todo: we should process case of compile error in fn main function
        std::process::exit(1);
    }

    // Exports the compiled project
    ExportTask::do_export(output.task, compiled, lock_dir).await?;

    Ok(())
}

/// Recompiles and exports on every change of a depended file.
async fn watch_main(
    verse: LspUniverse,
    task: ProjectTask,
    lock_dir: Option<ImmutPath>,
    on_export: Option<String>,
) -> Result<()> {
    let export_target = if matches!(task, ProjectTask::ExportHtml(..)) {
        ExportTarget::Html
    } else {
        ExportTarget::Paged
    };
    let opts = ProjectOpts {
        handle: Some(tokio::runtime::Handle::current()),
        analysis: Arc::new(Analysis::default()),
        config: Config::default(),
        export_target,
        #[cfg(feature = "preview")]
        preview: Default::default(),
    };

    // Exports run on their own task, so that compiling does not wait for them.
    let (artifact_tx, artifact_rx) = watch::channel(None);
    tokio::spawn(export_loop(artifact_rx, task, lock_dir, on_export));

    let result = start_project(verse, Some(opts), move |compiler, interrupt, next| {
        if let Interrupt::Compiled(artifact) = &interrupt {
            let diag = artifact.diagnostics();
            if let Err(err) = print_diagnostics(artifact.world(), diag, DiagnosticFormat::Human) {
                eprintln!("watch: cannot print diagnostics: {err:?}");
            }

            if artifact.has_errors() {
                eprintln!("compiled with errors");
            } else {
                artifact_tx.send_replace(Some(artifact.clone()));
            }
        }

        next(compiler, interrupt)
    });

    // There is no editor to answer these requests; discard them so that they
    // do not pile up in the channel.
    let mut editor_rx = result.editor_rx;
    tokio::spawn(async move { while editor_rx.recv().await.is_some() {} });

    result.service.run().await;
    Ok(())
}

/// Exports the newest artifact, one export at a time, so that an older export
/// cannot replace a newer one and `--on-export` commands do not overlap.
/// Artifacts that are replaced while an export runs are skipped.
async fn export_loop(
    mut artifacts: watch::Receiver<Option<LspCompiledArtifact>>,
    task: ProjectTask,
    lock_dir: Option<ImmutPath>,
    on_export: Option<String>,
) {
    while artifacts.changed().await.is_ok() {
        let Some(artifact) = artifacts.borrow_and_update().clone() else {
            continue;
        };

        match ExportTask::do_export(task.clone(), artifact, lock_dir.clone()).await {
            Ok(response) => {
                if let Some(command) = &on_export {
                    for path in exported_paths(response) {
                        run_on_export(command, path).await;
                    }
                }
                eprintln!("compiled successfully");
            }
            Err(err) => eprintln!("export failed: {err:?}"),
        }
    }
}

/// The files an export wrote.
fn exported_paths(response: Option<OnExportResponse>) -> Vec<PathBuf> {
    match response {
        Some(OnExportResponse::Single { path, .. }) => path.into_iter().collect(),
        Some(OnExportResponse::Paged { items, .. }) => {
            items.into_iter().filter_map(|item| item.path).collect()
        }
        None => vec![],
    }
}

/// Runs the `--on-export` command for one written file through the system
/// shell. Failures are reported on stderr and do not stop watching.
async fn run_on_export(command: &str, path: PathBuf) {
    let mut shell = shell_command(command, &path);
    shell.env("TINYMIST_OUTPUT", &path);

    // Waiting for the command blocks the thread.
    let status = tokio::task::spawn_blocking(move || shell.status()).await;

    match status {
        Ok(Ok(status)) if status.success() => {}
        Ok(Ok(status)) => eprintln!("on-export command failed with {status}: {command}"),
        Ok(Err(err)) => eprintln!("on-export command could not start: {err}: {command}"),
        Err(err) => eprintln!("on-export command panicked: {err}"),
    }
}

/// Makes the shell invocation that runs `command` with `path` as last argument.
#[cfg(not(windows))]
fn shell_command(command: &str, path: &Path) -> Command {
    // A positional parameter keeps the shell from interpreting the path.
    let mut shell = Command::new("sh");
    shell
        .arg("-c")
        .arg(format!("{command} \"$1\""))
        .arg("sh")
        .arg(path);
    shell
}

/// Makes the shell invocation that runs `command` with `path` as last argument.
#[cfg(windows)]
fn shell_command(command: &str, path: &Path) -> Command {
    use std::os::windows::process::CommandExt;

    // `cmd` does not understand the `\"` that `Command::arg` writes for quotes.
    // With `/S` it removes the outer pair of quotes and keeps the rest.
    let mut shell = Command::new("cmd");
    shell.raw_arg(format!("/S /C \"{command} \"{}\"\"", path.display()));
    shell
}
