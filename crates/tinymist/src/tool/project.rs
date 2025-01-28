//! Project management tools.

use std::path::{Path, PathBuf};

use reflexo::ImmutPath;
use tinymist_std::error::prelude::*;

use crate::{project::*, task::ExportTask};

/// Common arguments of compile, watch, and query.
#[derive(Debug, Clone, clap::Parser)]
pub struct CompileArgs {
    /// Inherits the compile task arguments.
    #[clap(flatten)]
    pub compile: TaskCompileArgs,

    /// Saves the compilation arguments to the lock file.
    #[clap(long)]
    pub save_lock: bool,

    /// Specifies the path to the lock file. If the path is
    /// set, the lock file will be saved.
    #[clap(long)]
    pub lockfile: Option<PathBuf>,
}

trait LockFileExt {
    fn preview(&mut self, doc_id: Id, args: &TaskPreviewArgs) -> Result<Id>;
}

impl LockFileExt for LockFile {
    fn preview(&mut self, doc_id: Id, args: &TaskPreviewArgs) -> Result<Id> {
        let task_id = args
            .name
            .as_ref()
            .map(|t| Id::new(t.clone()))
            .unwrap_or(doc_id.clone());

        let when = args.when.unwrap_or(TaskWhen::OnType);
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

/// Runs project compilation(s)
pub async fn compile_main(args: CompileArgs) -> Result<()> {
    // Identifies the input and output
    let input = args.compile.declare.to_input();
    let output = args.compile.to_task(input.id.clone())?;

    // Saves the lock file if the flags are set
    let save_lock = args.save_lock || args.lockfile.is_some();
    // todo: respect the name of the lock file
    let lock_dir: ImmutPath = if let Some(lockfile) = args.lockfile {
        lockfile.parent().context("no parent")?.into()
    } else {
        std::env::current_dir().context("lock directory")?.into()
    };

    if save_lock {
        LockFile::update(&lock_dir, |state| {
            state.replace_document(input.clone());
            state.replace_task(output.clone());

            Ok(())
        })?;
    }

    // Prepares for the compilation
    let universe = (input, lock_dir.clone()).resolve()?;
    let world = universe.snapshot();
    let snap = CompileSnapshot::from_world(world);

    // Compiles the project
    let compiled = snap.compile();

    // Exports the compiled project
    let lock_dir = save_lock.then_some(lock_dir);
    ExportTask::do_export(output.task, compiled, lock_dir).await?;

    Ok(())
}

/// Project document commands' main
pub fn project_main(args: DocCommands) -> Result<()> {
    LockFile::update(Path::new("."), |state| {
        match args {
            DocCommands::New(args) => {
                state.replace_document(args.to_input());
            }
            DocCommands::Configure(args) => {
                let id: Id = (&args.id).into();

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
pub fn task_main(args: TaskCommands) -> Result<()> {
    LockFile::update(Path::new("."), |state| {
        match args {
            TaskCommands::Preview(args) => {
                let input = args.declare.to_input();
                let id = input.id.clone();
                state.replace_document(input);
                let _ = state.preview(id, &args);
            }
        }

        Ok(())
    })
}
