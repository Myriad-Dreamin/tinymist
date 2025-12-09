//! Project management tools.

use std::sync::Arc;

use tinymist_query::analysis::Analysis;
use tokio::sync::mpsc;

use crate::project::*;
use crate::{actor::editor::EditorRequest, Config};

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

    let analysis = opts.analysis.clone();

    #[cfg(feature = "preview")]
    let preview = opts.preview;

    #[cfg(feature = "export")]
    let export_task =
        crate::task::ExportTask::new(handle, Some(editor_tx.clone()), opts.config.export());

    // Create the actor
    let compile_handle = CompileHandlerImpl::new(
        analysis,
        editor_tx.clone(),
        Arc::new(intr_tx.clone()),
        true,
        #[cfg(feature = "preview")]
        preview,
        #[cfg(feature = "export")]
        export_task,
    );

    let mut compiler = ProjectCompiler::new(
        verse,
        dep_tx,
        CompileServerOpts {
            handler: compile_handle,
            export_target: opts.export_target,
            syntax_only: opts.config.syntax_only,
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
