use std::path::Path;

#[cfg(feature = "l10n")]
use tinymist_std::error::prelude::*;

#[cfg(feature = "preview")]
use tinymist::tool::preview::PreviewArgs;
#[cfg(feature = "preview")]
use tinymist_project::DocNewArgs;
#[cfg(feature = "preview")]
use tinymist_project::LockFile;
#[cfg(feature = "preview")]
use tinymist_task::Id;
#[cfg(feature = "preview")]
use tinymist_task::TaskWhen;

/// Project task commands.
#[derive(Debug, Clone, clap::Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum TaskCommands {
    /// Declare a preview task.
    #[cfg(feature = "preview")]
    Preview(TaskPreviewArgs),
}

/// Declare an lsp task.
#[derive(Debug, Clone, clap::Parser)]
#[cfg(feature = "preview")]
pub struct TaskPreviewArgs {
    /// Argument to identify a project.
    #[clap(flatten)]
    pub declare: DocNewArgs,

    /// Name a task.
    #[clap(long = "task")]
    pub task_name: Option<String>,

    /// When to run the task
    #[arg(long = "when")]
    pub when: Option<TaskWhen>,

    /// Preview arguments
    #[clap(flatten)]
    pub preview: PreviewArgs,
}

/// Project task commands' main
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
