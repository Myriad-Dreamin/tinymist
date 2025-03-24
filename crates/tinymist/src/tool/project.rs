//! Project management tools.

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::Arc,
};

use clap_complete::Shell;
use parking_lot::Mutex;
use reflexo::{path::unix_slash, ImmutPath};
use reflexo_typst::WorldComputeGraph;
use tinymist_query::analysis::Analysis;
use tinymist_std::{bail, error::prelude::*};
use tokio::sync::mpsc;

use crate::{actor::editor::EditorRequest, world::system::print_diagnostics, Config};
use crate::{project::*, task::ExportTask};

/// Arguments for project compilation.
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

/// Arguments for generating a build script.
#[derive(Debug, Clone, clap::Parser)]
pub struct GenerateScriptArgs {
    /// The shell to generate the completion script for. If not provided, it
    /// will be inferred from the environment.
    #[clap(value_enum)]
    pub shell: Option<Shell>,
    /// The path to the output script.
    #[clap(short, long)]
    pub output: Option<String>,
}

#[cfg(feature = "preview")]
pub use typst_preview::{PreviewArgs, PreviewMode};

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

    /// Preview mode
    #[clap(long = "preview-mode", default_value = "document", value_name = "MODE")]
    pub preview_mode: PreviewMode,
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
    let graph = WorldComputeGraph::from_world(world);

    // Compiles the project
    let compiled = CompiledArtifact::from_graph(graph);

    let diag = compiled.diagnostics();
    print_diagnostics(compiled.world(), diag, DiagnosticFormat::Human)
        .context_ut("print diagnostics")?;

    if compiled.has_errors() {
        // todo: we should process case of compile error in fn main function
        std::process::exit(1);
    }

    // Exports the compiled project
    let lock_dir = save_lock.then_some(lock_dir);
    ExportTask::do_export(output.task, compiled, lock_dir).await?;

    Ok(())
}

/// Generates a build script for compilation
pub fn generate_script_main(args: GenerateScriptArgs) -> Result<()> {
    let Some(shell) = args.shell.or_else(Shell::from_env) else {
        bail!("could not infer shell");
    };
    let output = Path::new(args.output.as_deref().unwrap_or("build"));

    let output = match shell {
        Shell::Bash | Shell::Zsh | Shell::Elvish | Shell::Fish => output.with_extension("sh"),
        Shell::PowerShell => output.with_extension("ps1"),
        _ => bail!("unsupported shell: {shell:?}"),
    };

    let script = match shell {
        Shell::Bash | Shell::Zsh | Shell::PowerShell => shell_build_script(shell)?,
        _ => bail!("unsupported shell: {shell:?}"),
    };

    std::fs::write(output, script).context("write script")?;

    Ok(())
}

/// Generates a build script for shell-like shells
fn shell_build_script(shell: Shell) -> Result<String> {
    let mut output = String::new();

    match shell {
        Shell::Bash => {
            output.push_str("#!/usr/bin/env bash\n\n");
        }
        Shell::Zsh => {
            output.push_str("#!/usr/bin/env zsh\n\n");
        }
        Shell::PowerShell => {}
        _ => {}
    }

    let lock_dir = std::env::current_dir().context("current directory")?;

    let lock = LockFile::read(&lock_dir)?;

    struct CmdBuilder(Vec<Cow<'static, str>>);

    impl CmdBuilder {
        fn new() -> Self {
            Self(vec![])
        }

        fn extend(&mut self, args: impl IntoIterator<Item = impl Into<Cow<'static, str>>>) {
            for arg in args {
                self.0.push(arg.into());
            }
        }

        fn push(&mut self, arg: impl Into<Cow<'static, str>>) {
            self.0.push(arg.into());
        }

        fn build(self) -> String {
            self.0.join(" ")
        }
    }

    let quote_escape = |s: &str| s.replace("'", r#"'"'"'"#);
    let quote = |s: &str| format!("'{}'", s.replace("'", r#"'"'"'"#));

    let path_of = |p: &ResourcePath, loc: &str| {
        let Some(path) = p.to_rel_path(&lock_dir) else {
            log::error!("could not resolve path for {loc}, path: {p:?}");
            return String::default();
        };

        quote(&unix_slash(&path))
    };

    let base_cmd: Vec<&str> = vec!["tinymist", "compile", "--save-lock"];

    for task in lock.task.iter() {
        let Some(input) = lock.get_document(&task.document) else {
            log::warn!(
                "could not find document for task {:?}, whose document is {:?}",
                task.id,
                task.doc_id()
            );
            continue;
        };
        // todo: preview/query commands
        let Some(export) = task.task.as_export() else {
            continue;
        };

        let mut cmd = CmdBuilder::new();
        cmd.extend(base_cmd.iter().copied());
        cmd.push("--task");
        cmd.push(quote(&task.id.to_string()));

        cmd.push(path_of(&input.main, "main"));

        if let Some(root) = &input.root {
            cmd.push("--root");
            cmd.push(path_of(root, "root"));
        }

        for (k, v) in &input.inputs {
            cmd.push(format!(
                r#"--input='{}={}'"#,
                quote_escape(k),
                quote_escape(v)
            ));
        }

        for p in &input.font_paths {
            cmd.push("--font-path");
            cmd.push(path_of(p, "font-path"));
        }

        if !input.system_fonts {
            cmd.push("--ignore-system-fonts");
        }

        if let Some(p) = &input.package_path {
            cmd.push("--package-path");
            cmd.push(path_of(p, "package-path"));
        }

        if let Some(p) = &input.package_cache_path {
            cmd.push("--package-cache-path");
            cmd.push(path_of(p, "package-cache-path"));
        }

        if let Some(p) = &export.output {
            cmd.push("--output");
            cmd.push(quote(&p.to_string()));
        }

        for t in &export.transform {
            match t {
                ExportTransform::Pretty { .. } => {
                    cmd.push("--pretty");
                }
                ExportTransform::Pages { ranges } => {
                    for r in ranges {
                        cmd.push("--pages");
                        cmd.push(r.to_string());
                    }
                }
                // todo: export me
                ExportTransform::Merge { .. } | ExportTransform::Script { .. } => {}
            }
        }

        match &task.task {
            ProjectTask::Preview(..) | ProjectTask::Query(..) => {}
            ProjectTask::ExportPdf(task) => {
                cmd.push("--format=pdf");

                for s in &task.pdf_standards {
                    cmd.push("--pdf-standard");
                    let s = serde_json::to_string(s).context("pdf standard")?;
                    cmd.push(s);
                }

                if let Some(output) = &task.creation_timestamp {
                    cmd.push("--creation-timestamp");
                    cmd.push(output.to_string());
                }
            }
            ProjectTask::ExportSvg(..) => {
                cmd.push("--format=svg");
            }
            ProjectTask::ExportSvgHtml(..) => {
                cmd.push("--format=svg_html");
            }
            ProjectTask::ExportMd(..) => {
                cmd.push("--format=md");
            }
            ProjectTask::ExportPng(..) => {
                cmd.push("--format=png");
            }
            ProjectTask::ExportText(..) => {
                cmd.push("--format=txt");
            }
            ProjectTask::ExportHtml(..) => {
                cmd.push("--format=html");
            }
        }

        let ext = task.task.extension();

        output.push_str(&format!(
            "# From {} to {} ({ext})\n",
            task.doc_id(),
            task.id
        ));
        output.push_str(&cmd.build());
        output.push('\n');
    }

    Ok(output)
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
        let _ = state;
        match args {
            #[cfg(feature = "preview")]
            TaskCommands::Preview(args) => {
                let input = args.declare.to_input();
                let id = input.id.clone();
                state.replace_document(input);
                let _ = state.preview(id, &args);

                Ok(())
            }
        }
    })
}

#[derive(Default)]
pub(crate) struct ProjectOpts {
    /// The tokio runtime handle.
    pub handle: Option<tokio::runtime::Handle>,
    /// The shared preview state.
    pub analysis: Arc<Analysis>,
    /// The shared config.
    pub config: Config,
    /// The shared preview state.
    pub preview: ProjectPreviewState,
}

pub(crate) struct StartProjectResult<F> {
    pub service: WatchService<F>,
    pub intr_tx: mpsc::UnboundedSender<LspInterrupt>,
    pub editor_rx: mpsc::UnboundedReceiver<EditorRequest>,
}

// todo: This is only extracted from the `tinymist preview` command, and we need
// to abstract it in future.
/// Start a project with the given universe.
pub(crate) fn start_project<F>(
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
    let handle = opts.handle.unwrap_or_else(tokio::runtime::Handle::current);

    // type EditorSender = mpsc::UnboundedSender<EditorRequest>;
    let (editor_tx, editor_rx) = mpsc::unbounded_channel();
    let (intr_tx, intr_rx) = tokio::sync::mpsc::unbounded_channel();

    // todo: unify filesystem watcher
    let (dep_tx, dep_rx) = tokio::sync::mpsc::unbounded_channel();
    let fs_intr_tx = intr_tx.clone();
    handle.spawn(watch_deps(dep_rx, move |event| {
        fs_intr_tx.interrupt(LspInterrupt::Fs(event));
    }));

    // Create the actor
    let compile_handle = Arc::new(CompileHandlerImpl {
        preview: opts.preview,
        is_standalone: true,
        export: crate::task::ExportTask::new(handle, Some(editor_tx.clone()), opts.config.export()),
        editor_tx,
        client: Box::new(intr_tx.clone()),

        analysis: opts.analysis,
        status_revision: Mutex::default(),
        notified_revision: Mutex::default(),
    });

    let mut compiler = ProjectCompiler::new(
        verse,
        dep_tx,
        CompileServerOpts {
            handler: compile_handle,
            enable_watch: true,
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

pub(crate) struct WatchService<F> {
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
