//! Project management tools.

use std::path::PathBuf;

use tinymist::ExportTask;
use tinymist::project::*;
use tinymist::world::WorldComputeGraph;
use tinymist::world::system::print_diagnostics;
use tinymist_std::{ImmutPath, error::prelude::*};

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
    let doc_id = input.id.clone();
    let output = args.compile.to_task(doc_id.clone(), &cwd)?;

    if save_lock {
        LockFile::update(&lock_dir, |state| {
            state.replace_document(input.relative_to(&lock_dir));
            state.replace_task(output.clone());

            Ok(())
        })?;
    }

    // Prepares for the compilation
    let universe = (input, lock_dir.clone()).resolve()?;
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

    // Records the route and dependencies of the document saved above
    if save_lock {
        let mut updater = update_lock(lock_dir);
        updater.update_materials(doc_id.clone(), compiled.world().depended_fs_paths());
        updater.route(doc_id, PROJECT_ROUTE_USER_ACTION_PRIORITY);
        updater.commit();
    }

    // Exports the compiled project
    ExportTask::do_export(output.task, compiled).await?;

    Ok(())
}
