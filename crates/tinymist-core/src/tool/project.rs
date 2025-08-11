//! Project management tools.

use std::sync::Arc;

use parking_lot::Mutex;
use tinymist_query::analysis::Analysis;
use tokio::sync::mpsc;

use crate::project::*;
use crate::{actor::editor::EditorRequest, Config};

#[cfg(all(feature = "system", feature = "lock"))]
use std::path::Path;

#[cfg(feature = "system")]
use tinymist_std::error::prelude::*;

#[cfg(feature = "preview")]
pub use super::preview::PreviewArgs;
#[cfg(feature = "preview")]
pub use tinymist_preview::PreviewMode;

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

#[cfg(feature = "preview")]
trait LockFileExt {
    fn preview(&mut self, doc_id: Id, args: &TaskPreviewArgs) -> Result<Id>;
}

#[cfg(feature = "preview")]
impl LockFileExt for LockFile {
    fn preview(&mut self, doc_id: Id, args: &TaskPreviewArgs) -> Result<Id> {
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
#[cfg(all(feature = "system", feature = "lock"))]
pub fn project_main(args: DocCommands) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot get cwd")?;
    LockFile::update(&cwd, |state| {
        let ctx: (&Path, &Path) = (&cwd, &cwd);
        match args {
            DocCommands::New(args) => {
                state.replace_document(args.to_input(ctx));
            }
            DocCommands::Configure(args) => {
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
#[cfg(all(feature = "system", feature = "lock"))]
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

/// Options for starting a project.
#[derive(Default)]
pub struct ProjectOpts {
    /// The tokio runtime handle.
    pub handle: Option<tokio::runtime::Handle>,
    /// The shared preview state.
    pub analysis: Arc<Analysis>,
    /// The shared config.
    pub config: Config,
    /// The shared preview state.
    #[cfg(feature = "preview")]
    pub preview: ProjectPreviewState,
    /// The export target.
    pub export_target: ExportTarget,
}

/// Result of starting a project.
pub struct StartProjectResult<F> {
    /// A future service that runs the project.
    pub service: WatchService<F>,
    /// The interrupt sender.
    pub intr_tx: mpsc::UnboundedSender<LspInterrupt>,
    /// The editor request receiver.
    pub editor_rx: mpsc::UnboundedReceiver<EditorRequest>,
}

// todo: This is only extracted from the `tinymist preview` command, and we need
// to abstract it in future.
/// Starts a project with the given universe.
pub fn start_project<F>(
    verse: LspUniverse,
    opts: Option<ProjectOpts>,
    intr_handler: F,
) -> StartProjectResult<F>
where
    F: FnMut(
        &mut LspProjectCompiler,
        Interrupt<LspCompilerFeat>,
        fn(&mut LspProjectCompiler, Interrupt<LspCompilerFeat>),
    ),
{
    let opts = opts.unwrap_or_default();
    #[cfg(any(feature = "export", feature = "system"))]
    let handle = opts.handle.unwrap_or_else(tokio::runtime::Handle::current);

    let _ = opts.config;

    // type EditorSender = mpsc::UnboundedSender<EditorRequest>;
    let (editor_tx, editor_rx) = mpsc::unbounded_channel();
    let (intr_tx, intr_rx) = tokio::sync::mpsc::unbounded_channel();

    // todo: unify filesystem watcher
    let (dep_tx, dep_rx) = mpsc::unbounded_channel();
    // todo: notify feature?
    #[cfg(feature = "system")]
    {
        let fs_intr_tx = intr_tx.clone();
        handle.spawn(watch_deps(dep_rx, move |event| {
            fs_intr_tx.interrupt(LspInterrupt::Fs(event));
        }));
    }
    #[cfg(not(feature = "system"))]
    {
        let _ = dep_rx;
        log::warn!("Project: system watcher is not enabled, file changes will not be watched");
    }

    // Create the actor
    let compile_handle = Arc::new(CompileHandlerImpl {
        #[cfg(feature = "preview")]
        preview: opts.preview,
        is_standalone: true,
        #[cfg(feature = "export")]
        export: crate::task::ExportTask::new(handle, Some(editor_tx.clone()), opts.config.export()),
        editor_tx,
        client: Arc::new(intr_tx.clone()),

        analysis: opts.analysis,
        status_revision: Mutex::default(),
        notified_revision: Mutex::default(),
    });

    let mut compiler = ProjectCompiler::new(
        verse,
        dep_tx,
        CompileServerOpts {
            handler: compile_handle,
            export_target: opts.export_target,
            ignore_first_sync: true,
        },
    );

    compiler.primary.reason.by_entry_update = true;

    StartProjectResult {
        service: WatchService {
            compiler,
            intr_rx,
            intr_handler,
        },
        intr_tx,
        editor_rx,
    }
}

/// A service that watches for project changes and compiles them.
pub struct WatchService<F> {
    /// The project compiler.
    pub compiler: LspProjectCompiler,
    intr_rx: tokio::sync::mpsc::UnboundedReceiver<LspInterrupt>,
    intr_handler: F,
}

impl<F> WatchService<F>
where
    F: FnMut(
            &mut LspProjectCompiler,
            Interrupt<LspCompilerFeat>,
            fn(&mut LspProjectCompiler, Interrupt<LspCompilerFeat>),
        ) + Send
        + 'static,
{
    /// Runs the project service.
    pub async fn run(self) {
        let Self {
            mut compiler,
            mut intr_rx,
            mut intr_handler,
        } = self;

        let handler = compiler.handler.clone();
        handler.on_any_compile_reason(&mut compiler);

        while let Some(intr) = intr_rx.recv().await {
            log::debug!("Project compiler received: {intr:?}");
            intr_handler(&mut compiler, intr, ProjectState::do_interrupt);
        }

        log::info!("Project compiler exited");
    }
}
