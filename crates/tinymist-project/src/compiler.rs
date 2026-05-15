//! Project compiler for tinymist.

use core::fmt;
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use ecow::{EcoString, EcoVec, eco_vec};
use tinymist_std::error::prelude::Result;
use tinymist_std::{ImmutPath, typst::TypstDocument};
use tinymist_task::ExportTarget;
use tinymist_world::vfs::notify::{
    FilesystemEvent, MemoryEvent, NotifyDeps, NotifyMessage, UpstreamUpdateEvent,
};
use tinymist_world::vfs::{FileId, FsProvider, RevisingVfs, WorkspaceResolver};
use tinymist_world::{
    CompileSignal, CompileSnapshot, CompilerFeat, CompilerUniverse, DiagnosticsTask, EntryReader,
    EntryState, FlagTask, HtmlCompilationTask, PagedCompilationTask, ProjectInsId, TaskInputs,
    WorldComputeGraph, WorldDeps,
};
use tokio::sync::mpsc;
use typst::World;
use typst::diag::{At, FileError};
use typst::syntax::Span;

/// A compiled artifact.
pub struct CompiledArtifact<F: CompilerFeat> {
    /// The used compute graph.
    pub graph: Arc<WorldComputeGraph<F>>,
    /// The diagnostics of the document.
    pub diag: Arc<DiagnosticsTask>,
    /// The compiled document.
    pub doc: Option<TypstDocument>,
    /// The depended files.
    pub deps: OnceLock<EcoVec<FileId>>,
}

impl<F: CompilerFeat> fmt::Display for CompiledArtifact<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rev = self.graph.snap.world.revision();
        write!(f, "CompiledArtifact({:?}, rev={rev:?})", self.graph.snap.id)
    }
}

impl<F: CompilerFeat> std::ops::Deref for CompiledArtifact<F> {
    type Target = Arc<WorldComputeGraph<F>>;

    fn deref(&self) -> &Self::Target {
        &self.graph
    }
}

impl<F: CompilerFeat> Clone for CompiledArtifact<F> {
    fn clone(&self) -> Self {
        Self {
            graph: self.graph.clone(),
            doc: self.doc.clone(),
            diag: self.diag.clone(),
            deps: self.deps.clone(),
        }
    }
}

impl<F: CompilerFeat> CompiledArtifact<F> {
    /// Returns the project id.
    pub fn id(&self) -> &ProjectInsId {
        &self.graph.snap.id
    }

    /// Returns the last successfully compiled document.
    pub fn success_doc(&self) -> Option<TypstDocument> {
        self.doc
            .as_ref()
            .cloned()
            .or_else(|| self.snap.success_doc.clone())
    }

    /// Returns the depended files.
    pub fn depended_files(&self) -> &EcoVec<FileId> {
        self.deps.get_or_init(|| {
            let mut deps = EcoVec::default();
            self.graph.snap.world.iter_dependencies(&mut |f| {
                deps.push(f);
            });

            deps
        })
    }

    /// Runs the compiler and returns the compiled document.
    pub fn from_graph(graph: Arc<WorldComputeGraph<F>>, is_html: bool) -> CompiledArtifact<F> {
        let _ = graph.provide::<FlagTask<HtmlCompilationTask>>(Ok(FlagTask::flag(is_html)));
        let _ = graph.provide::<FlagTask<PagedCompilationTask>>(Ok(FlagTask::flag(!is_html)));
        let doc = if is_html {
            graph.shared_compile_html().expect("html").map(From::from)
        } else {
            graph.shared_compile().expect("paged").map(From::from)
        };

        CompiledArtifact {
            diag: graph.shared_diagnostics().expect("diag"),
            graph,
            doc,
            deps: OnceLock::default(),
        }
    }

    /// Returns the error count.
    pub fn error_cnt(&self) -> usize {
        self.diag.error_cnt()
    }

    /// Returns the warning count.
    pub fn warning_cnt(&self) -> usize {
        self.diag.warning_cnt()
    }

    /// Returns the diagnostics.
    pub fn diagnostics(&self) -> impl Iterator<Item = &typst::diag::SourceDiagnostic> + Clone {
        self.diag.diagnostics()
    }

    /// Returns whether there are any errors.
    pub fn has_errors(&self) -> bool {
        self.error_cnt() > 0
    }

    /// Sets the signal.
    pub fn with_signal(mut self, signal: CompileSignal) -> Self {
        let mut snap = self.snap.clone();
        snap.signal = signal;

        self.graph = self.graph.snapshot_unsafe(snap);
        self
    }
}

/// The compilation status of a project.
#[derive(Debug, Clone)]
pub struct CompileReport {
    /// The project ID.
    pub id: ProjectInsId,
    /// The file getting compiled.
    pub compiling_id: Option<FileId>,
    /// The number of pages in the compiled document, zero if failed.
    pub page_count: u32,
    /// The status of the compilation.
    pub status: CompileStatusEnum,
}

/// The compilation status of a project.
#[derive(Debug, Clone)]
pub enum CompileStatusEnum {
    /// The project is suspended.
    Suspend,
    /// The project is compiling.
    Compiling,
    /// The project compiled successfully.
    CompileSuccess(CompileStatusResult),
    /// The project failed to compile.
    CompileError(CompileStatusResult),
    /// The project failed to export.
    ExportError(CompileStatusResult),
}

/// The compilation status result of a project.
#[derive(Debug, Clone)]
pub struct CompileStatusResult {
    /// The number of errors or warnings occur.
    diag: u32,
    /// Used time
    elapsed: tinymist_std::time::Duration,
}

impl CompileReport {
    /// Gets the status message.
    pub fn message(&self) -> CompileReportMsg<'_> {
        CompileReportMsg(self)
    }
}

/// A message of the compilation status.
pub struct CompileReportMsg<'a>(&'a CompileReport);

impl fmt::Display for CompileReportMsg<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use CompileStatusEnum::*;
        use CompileStatusResult as Res;

        let input = WorkspaceResolver::display(self.0.compiling_id);
        let (stage, Res { diag, elapsed }) = match &self.0.status {
            Suspend => return f.write_str("suspended"),
            Compiling => return f.write_str("compiling"),
            CompileSuccess(Res { diag: 0, elapsed }) => {
                return write!(f, "{input:?}: compilation succeeded in {elapsed:?}");
            }
            CompileSuccess(res) => ("compilation succeeded", res),
            CompileError(res) => ("compilation failed", res),
            ExportError(res) => ("export failed", res),
        };
        write!(
            f,
            "{input:?}: {stage} with {diag} warnings and errors in {elapsed:?}"
        )
    }
}

/// A project compiler handler.
pub trait CompileHandler<F: CompilerFeat, Ext>: Send + Sync + 'static {
    /// Called when there is any reason to compile. This doesn't mean that the
    /// project should be compiled.
    fn on_any_compile_reason(&self, state: &mut ProjectCompiler<F, Ext>);
    // todo: notify project specific compile
    /// Called when a compilation is done.
    fn notify_compile(&self, res: &CompiledArtifact<F>);
    /// Called when a project is removed.
    fn notify_removed(&self, _id: &ProjectInsId) {}
    /// Called when the compilation status is changed.
    fn status(&self, revision: usize, rep: CompileReport);
}

/// No need so no compilation.
impl<F: CompilerFeat + Send + Sync + 'static, Ext: 'static> CompileHandler<F, Ext>
    for std::marker::PhantomData<fn(F, Ext)>
{
    fn on_any_compile_reason(&self, _state: &mut ProjectCompiler<F, Ext>) {
        log::info!("ProjectHandle: no need to compile");
    }
    fn notify_compile(&self, _res: &CompiledArtifact<F>) {}
    fn status(&self, _revision: usize, _rep: CompileReport) {}
}

/// An interrupt to the compiler.
pub enum Interrupt<F: CompilerFeat> {
    /// Compile anyway.
    Compile(ProjectInsId),
    /// Settle a dedicated project.
    Settle(ProjectInsId),
    /// Compiled from computing thread.
    Compiled(CompiledArtifact<F>),
    /// Change the watching entry.
    ChangeTask(ProjectInsId, TaskInputs),
    /// Font changes.
    Font(Arc<F::FontResolver>),
    /// Creation timestamp changes.
    CreationTimestamp(Option<i64>),
    /// Memory file changes.
    Memory(MemoryEvent),
    /// File system event.
    Fs(FilesystemEvent),
    /// Save a file.
    Save(ImmutPath),
}

impl<F: CompilerFeat> fmt::Debug for Interrupt<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Interrupt::Compile(id) => write!(f, "Compile({id:?})"),
            Interrupt::Settle(id) => write!(f, "Settle({id:?})"),
            Interrupt::Compiled(artifact) => write!(f, "Compiled({:?})", artifact.id()),
            Interrupt::ChangeTask(id, change) => {
                write!(f, "ChangeTask({id:?}, entry={:?})", change.entry.is_some())
            }
            Interrupt::Font(..) => write!(f, "Font(..)"),
            Interrupt::CreationTimestamp(ts) => write!(f, "CreationTimestamp({ts:?})"),
            Interrupt::Memory(..) => write!(f, "Memory(..)"),
            Interrupt::Fs(..) => write!(f, "Fs(..)"),
            Interrupt::Save(path) => write!(f, "Save({path:?})"),
        }
    }
}

fn no_reason() -> CompileSignal {
    CompileSignal::default()
}

fn reason_by_mem() -> CompileSignal {
    CompileSignal {
        by_mem_events: true,
        ..CompileSignal::default()
    }
}

fn reason_by_fs() -> CompileSignal {
    CompileSignal {
        by_fs_events: true,
        ..CompileSignal::default()
    }
}

fn reason_by_entry_change() -> CompileSignal {
    CompileSignal {
        by_entry_update: true,
        ..CompileSignal::default()
    }
}

/// A tagged memory event with logical tick.
struct TaggedMemoryEvent {
    /// The logical tick when the event is received.
    logical_tick: usize,
    /// The memory event happened.
    event: MemoryEvent,
}

/// The compiler server options.
pub struct CompileServerOpts<F: CompilerFeat, Ext> {
    /// The compilation handler.
    pub handler: Arc<dyn CompileHandler<F, Ext>>,
    /// Whether to ignoring the first fs sync event.
    pub ignore_first_sync: bool,
    /// Specifies the current export target.
    pub export_target: ExportTarget,
    /// Whether to run in syntax-only mode.
    pub syntax_only: bool,
}

impl<F: CompilerFeat + Send + Sync + 'static, Ext: 'static> Default for CompileServerOpts<F, Ext> {
    fn default() -> Self {
        Self {
            handler: Arc::new(std::marker::PhantomData),
            ignore_first_sync: false,
            syntax_only: false,
            export_target: ExportTarget::Paged,
        }
    }
}

const FILE_MISSING_ERROR_MSG: EcoString = EcoString::inline("t-file-missing");
/// The file missing error constant.
pub const FILE_MISSING_ERROR: FileError = FileError::Other(Some(FILE_MISSING_ERROR_MSG));

/// The synchronous compiler that runs on one project or multiple projects.
pub struct ProjectCompiler<F: CompilerFeat, Ext> {
    /// The compilation handle.
    pub handler: Arc<dyn CompileHandler<F, Ext>>,
    /// Specifies the current export target.
    export_target: ExportTarget,
    /// Whether to run in syntax-only mode.
    syntax_only: bool,
    /// Channel for sending interrupts to the compiler actor.
    dep_tx: mpsc::UnboundedSender<NotifyMessage>,
    /// Whether to ignore the first sync event.
    pub ignore_first_sync: bool,

    /// The current logical tick.
    logical_tick: usize,
    /// Last logical tick when invalidation is caused by shadow update.
    dirty_shadow_logical_tick: usize,
    /// Estimated latest set of shadow files.
    estimated_shadow_files: HashSet<Arc<Path>>,

    /// The primary state.
    pub primary: ProjectInsState<F, Ext>,
    /// The states for dedicate tasks
    pub dedicates: Vec<ProjectInsState<F, Ext>>,
    /// The project file dependencies.
    deps: ProjectDeps,
}

impl<F: CompilerFeat + Send + Sync + 'static, Ext: Default + 'static> ProjectCompiler<F, Ext> {
    /// Creates a compiler with options
    pub fn new(
        verse: CompilerUniverse<F>,
        dep_tx: mpsc::UnboundedSender<NotifyMessage>,
        CompileServerOpts {
            handler,
            ignore_first_sync,
            export_target,
            syntax_only,
        }: CompileServerOpts<F, Ext>,
    ) -> Self {
        let primary = Self::create_project(
            ProjectInsId("primary".into()),
            verse,
            export_target,
            syntax_only,
            handler.clone(),
        );
        Self {
            handler,
            dep_tx,
            export_target,
            syntax_only,

            logical_tick: 1,
            dirty_shadow_logical_tick: 0,

            estimated_shadow_files: Default::default(),
            ignore_first_sync,

            primary,
            deps: Default::default(),
            dedicates: vec![],
        }
    }

    /// Creates a snapshot of the primary project.
    pub fn snapshot(&mut self) -> Arc<WorldComputeGraph<F>> {
        self.primary.snapshot()
    }

    /// Compiles the document once.
    pub fn compile_once(&mut self) -> CompiledArtifact<F> {
        let snap = self.primary.make_snapshot();
        ProjectInsState::run_compile(
            self.handler.clone(),
            snap,
            self.export_target,
            self.syntax_only,
        )()
    }

    /// Gets the iterator of all projects.
    pub fn projects(&mut self) -> impl Iterator<Item = &mut ProjectInsState<F, Ext>> {
        std::iter::once(&mut self.primary).chain(self.dedicates.iter_mut())
    }

    fn create_project(
        id: ProjectInsId,
        verse: CompilerUniverse<F>,
        export_target: ExportTarget,
        syntax_only: bool,
        handler: Arc<dyn CompileHandler<F, Ext>>,
    ) -> ProjectInsState<F, Ext> {
        ProjectInsState {
            id,
            ext: Default::default(),
            syntax_only,
            verse,
            reason: no_reason(),
            cached_snapshot: None,
            handler,
            export_target,
            latest_compilation: OnceLock::default(),
            latest_success_doc: None,
            deps: Default::default(),
            committed_revision: 0,
        }
    }

    /// Find a project by id, but with less borrow checker restriction.
    pub fn find_project<'a>(
        primary: &'a mut ProjectInsState<F, Ext>,
        dedicates: &'a mut [ProjectInsState<F, Ext>],
        id: &ProjectInsId,
    ) -> &'a mut ProjectInsState<F, Ext> {
        if id == &primary.id {
            return primary;
        }

        dedicates.iter_mut().find(|e| e.id == *id).unwrap()
    }

    /// Clear all dedicate projects.
    pub fn clear_dedicates(&mut self) {
        self.dedicates.clear();
    }

    /// Restart a dedicate project.
    pub fn restart_dedicate(&mut self, group: &str, entry: EntryState) -> Result<ProjectInsId> {
        let id = ProjectInsId(group.into());

        let verse = CompilerUniverse::<F>::new_raw(
            entry,
            self.primary.verse.features.clone(),
            Some(self.primary.verse.inputs().clone()),
            self.primary.verse.vfs().fork(),
            self.primary.verse.registry.clone(),
            self.primary.verse.font_resolver.clone(),
            self.primary.verse.creation_timestamp,
        );

        let mut proj = Self::create_project(
            id.clone(),
            verse,
            self.export_target,
            self.syntax_only,
            self.handler.clone(),
        );
        proj.reason.merge(reason_by_entry_change());

        self.remove_dedicates(&id);
        self.dedicates.push(proj);

        Ok(id)
    }

    fn remove_dedicates(&mut self, id: &ProjectInsId) {
        let proj = self.dedicates.iter().position(|e| e.id == *id);
        if let Some(idx) = proj {
            // Resets the handle state, e.g. notified revision
            self.handler.notify_removed(id);
            self.deps.project_deps.remove_mut(id);

            let _proj = self.dedicates.remove(idx);
            // todo: kill compilations

            let res = self
                .dep_tx
                .send(NotifyMessage::SyncDependency(Box::new(self.deps.clone())));
            log_send_error("dep_tx", res);
        } else {
            log::warn!("ProjectCompiler: settle project not found {id:?}");
        }
    }

    /// Process an interrupt.
    pub fn process(&mut self, intr: Interrupt<F>) {
        // todo: evcit cache
        self.process_inner(intr);
        // Customized Project Compilation Handler
        self.handler.clone().on_any_compile_reason(self);
    }

    fn process_inner(&mut self, intr: Interrupt<F>) {
        match intr {
            Interrupt::Compile(id) => {
                let proj = Self::find_project(&mut self.primary, &mut self.dedicates, &id);
                // Increment the revision anyway.
                proj.verse.increment_revision(|verse| {
                    verse.flush();
                });

                proj.reason.merge(reason_by_entry_change());
            }
            Interrupt::Compiled(artifact) => {
                let proj =
                    Self::find_project(&mut self.primary, &mut self.dedicates, artifact.id());

                let processed = proj.process_compile(artifact);

                if processed {
                    self.deps
                        .project_deps
                        .insert_mut(proj.id.clone(), proj.deps.clone());

                    let event = NotifyMessage::SyncDependency(Box::new(self.deps.clone()));
                    let err = self.dep_tx.send(event);
                    log_send_error("dep_tx", err);
                }
            }
            Interrupt::Settle(id) => {
                self.remove_dedicates(&id);
            }
            Interrupt::ChangeTask(id, change) => {
                let proj = Self::find_project(&mut self.primary, &mut self.dedicates, &id);
                proj.verse.increment_revision(|verse| {
                    if let Some(inputs) = change.inputs.clone() {
                        verse.set_inputs(inputs);
                    }

                    if let Some(entry) = change.entry.clone() {
                        let res = verse.mutate_entry(entry);
                        if let Err(err) = res {
                            log::error!("ProjectCompiler: change entry error: {err:?}");
                        }
                    }
                });

                // After incrementing the revision
                if let Some(entry) = change.entry {
                    // todo: dedicate suspended
                    if entry.is_inactive() {
                        log::info!("ProjectCompiler: removing diag");
                        self.handler.status(proj.verse.revision.get(), {
                            CompileReport {
                                id: proj.id.clone(),
                                compiling_id: None,
                                page_count: 0,
                                status: CompileStatusEnum::Suspend,
                            }
                        });
                    }

                    // Forget the document state of previous entry.
                    proj.latest_success_doc = None;
                }

                proj.reason.merge(reason_by_entry_change());
            }

            Interrupt::Font(fonts) => {
                self.projects().for_each(|proj| {
                    let font_changed = proj.verse.increment_revision(|verse| {
                        verse.set_fonts(fonts.clone());
                        verse.font_changed()
                    });
                    if font_changed {
                        // todo: reason_by_font_change
                        proj.reason.merge(reason_by_entry_change());
                    }
                });
            }
            Interrupt::CreationTimestamp(creation_timestamp) => {
                self.projects().for_each(|proj| {
                    let timestamp_changed = proj.verse.increment_revision(|verse| {
                        verse.set_creation_timestamp(creation_timestamp);
                        // Creation timestamp changes affect compilation
                        verse.creation_timestamp_changed()
                    });
                    if timestamp_changed {
                        proj.reason.merge(reason_by_entry_change());
                    }
                });
            }
            Interrupt::Memory(event) => {
                log::debug!("ProjectCompiler: memory event incoming");

                // Emulate memory changes.
                let mut files = HashSet::new();
                if matches!(event, MemoryEvent::Sync(..)) {
                    std::mem::swap(&mut files, &mut self.estimated_shadow_files);
                }

                let (MemoryEvent::Sync(e) | MemoryEvent::Update(e)) = &event;
                for path in &e.removes {
                    self.estimated_shadow_files.remove(path);
                    files.insert(Arc::clone(path));
                }
                for (path, _) in &e.inserts {
                    self.estimated_shadow_files.insert(Arc::clone(path));
                    files.remove(path);
                }

                // If there is no invalidation happening, apply memory changes directly.
                if files.is_empty() && self.dirty_shadow_logical_tick == 0 {
                    let changes = std::iter::repeat_n(event, 1 + self.dedicates.len());
                    let proj = std::iter::once(&mut self.primary).chain(self.dedicates.iter_mut());
                    for (proj, event) in proj.zip(changes) {
                        log::debug!("memory update: vfs {:#?}", proj.verse.vfs().display());
                        let vfs_changed = proj.verse.increment_revision(|verse| {
                            log::debug!("memory update: {:?}", proj.id);
                            Self::apply_memory_changes(&mut verse.vfs(), event.clone());
                            log::debug!("memory update: changed {}", verse.vfs_changed());
                            verse.vfs_changed()
                        });
                        if vfs_changed {
                            proj.reason.merge(reason_by_mem());
                        }
                        log::debug!("memory update: vfs after {:#?}", proj.verse.vfs().display());
                    }
                    return;
                }

                // Otherwise, send upstream update event.
                // Also, record the logical tick when shadow is dirty.
                self.dirty_shadow_logical_tick = self.logical_tick;
                let event = NotifyMessage::UpstreamUpdate(UpstreamUpdateEvent {
                    invalidates: files.into_iter().collect(),
                    opaque: Box::new(TaggedMemoryEvent {
                        logical_tick: self.logical_tick,
                        event,
                    }),
                });
                let err = self.dep_tx.send(event);
                log_send_error("dep_tx", err);
            }
            Interrupt::Save(event) => {
                let changes = std::iter::repeat_n(&event, 1 + self.dedicates.len());
                let proj = std::iter::once(&mut self.primary).chain(self.dedicates.iter_mut());

                for (proj, saved_path) in proj.zip(changes) {
                    log::debug!(
                        "ProjectCompiler({}, rev={}): save changes",
                        proj.verse.revision.get(),
                        proj.id
                    );

                    // todo: only emit if saved_path is related
                    let _ = saved_path;

                    proj.reason.merge(reason_by_fs());
                }
            }
            Interrupt::Fs(event) => {
                log::debug!("ProjectCompiler: fs event incoming {event:?}");

                // Apply file system changes.
                let dirty_tick = &mut self.dirty_shadow_logical_tick;
                let (changes, is_sync, event) = event.split_with_is_sync();
                let changes = std::iter::repeat_n(changes, 1 + self.dedicates.len());
                let proj = std::iter::once(&mut self.primary).chain(self.dedicates.iter_mut());

                for (proj, changes) in proj.zip(changes) {
                    log::debug!(
                        "ProjectCompiler({}, rev={}): fs changes applying",
                        proj.verse.revision.get(),
                        proj.id
                    );

                    proj.verse.increment_revision(|verse| {
                        let mut vfs = verse.vfs();

                        // Handle delayed upstream update event before applying file system
                        // changes
                        if Self::apply_delayed_memory_changes(&mut vfs, dirty_tick, &event)
                            .is_none()
                        {
                            log::warn!("ProjectCompiler: unknown upstream update event");

                            // Actual a delayed memory event.
                            proj.reason.merge(reason_by_mem());
                        }
                        vfs.notify_fs_changes(changes);
                    });

                    log::debug!(
                        "ProjectCompiler({},rev={}): fs changes applied, {is_sync}",
                        proj.id,
                        proj.verse.revision.get(),
                    );

                    if !self.ignore_first_sync || !is_sync {
                        proj.reason.merge(reason_by_fs());
                    }
                }
            }
        }
    }

    /// Apply delayed memory changes to underlying compiler.
    fn apply_delayed_memory_changes(
        verse: &mut RevisingVfs<'_, F::AccessModel>,
        dirty_shadow_logical_tick: &mut usize,
        event: &Option<UpstreamUpdateEvent>,
    ) -> Option<()> {
        // Handle delayed upstream update event before applying file system changes
        if let Some(event) = event {
            let TaggedMemoryEvent {
                logical_tick,
                event,
            } = event.opaque.as_ref().downcast_ref()?;

            // Recovery from dirty shadow state.
            if logical_tick == dirty_shadow_logical_tick {
                *dirty_shadow_logical_tick = 0;
            }

            Self::apply_memory_changes(verse, event.clone());
        }

        Some(())
    }

    /// Apply memory changes to underlying compiler.
    fn apply_memory_changes(vfs: &mut RevisingVfs<'_, F::AccessModel>, event: MemoryEvent) {
        if matches!(event, MemoryEvent::Sync(..)) {
            vfs.reset_shadow();
        }
        match event {
            MemoryEvent::Update(event) | MemoryEvent::Sync(event) => {
                for path in event.removes {
                    let _ = vfs.unmap_shadow(&path);
                }
                for (path, snap) in event.inserts {
                    let _ = vfs.map_shadow(&path, snap);
                }
            }
        }
    }
}

/// A project instance state.
pub struct ProjectInsState<F: CompilerFeat, Ext> {
    /// The project instance id.
    pub id: ProjectInsId,
    /// The extension
    pub ext: Ext,
    /// The underlying universe.
    pub verse: CompilerUniverse<F>,
    /// Specifies the current export target.
    pub export_target: ExportTarget,
    /// Whether to run in syntax-only mode.
    pub syntax_only: bool,
    /// The reason to compile.
    pub reason: CompileSignal,
    /// The compilation handle.
    pub handler: Arc<dyn CompileHandler<F, Ext>>,
    /// The file dependencies.
    deps: EcoVec<ImmutPath>,

    /// The latest compute graph (snapshot), derived lazily from
    /// `latest_compilation` as needed.
    pub cached_snapshot: Option<Arc<WorldComputeGraph<F>>>,
    /// The latest compilation.
    pub latest_compilation: OnceLock<CompiledArtifact<F>>,
    /// The latest successly compiled document.
    pub latest_success_doc: Option<TypstDocument>,

    committed_revision: usize,
}

impl<F: CompilerFeat, Ext: 'static> ProjectInsState<F, Ext> {
    /// Gets a snapshot of the project.
    pub fn snapshot(&mut self) -> Arc<WorldComputeGraph<F>> {
        match self.cached_snapshot.as_ref() {
            Some(snap) if snap.world().revision() == self.verse.revision => snap.clone(),
            _ => {
                let snap = self.make_snapshot();
                self.cached_snapshot = Some(snap.clone());
                snap
            }
        }
    }

    /// Creates a new snapshot of the project derived from `latest_compilation`.
    fn make_snapshot(&self) -> Arc<WorldComputeGraph<F>> {
        let world = self.verse.snapshot();
        let snap = CompileSnapshot {
            id: self.id.clone(),
            world,
            signal: self.reason,
            success_doc: self.latest_success_doc.clone(),
        };
        WorldComputeGraph::new(snap)
    }

    /// Compiles the document once if there is any reason and the entry is
    /// active. (this is used for experimenting typst.node compilations)
    #[must_use]
    pub fn may_compile2<'a>(
        &mut self,
        compute: impl FnOnce(&Arc<WorldComputeGraph<F>>) + 'a,
    ) -> Option<impl FnOnce() -> Arc<WorldComputeGraph<F>> + 'a> {
        if !self.reason.any() || self.verse.entry_state().is_inactive() {
            return None;
        }

        let snap = self.snapshot();
        self.reason = Default::default();
        Some(move || {
            compute(&snap);
            snap
        })
    }

    /// Compiles the document once if there is any reason and the entry is
    /// active.
    #[must_use]
    pub fn may_compile(
        &mut self,
        handler: &Arc<dyn CompileHandler<F, Ext>>,
    ) -> Option<impl FnOnce() -> CompiledArtifact<F> + 'static> {
        if !self.reason.any() || self.verse.entry_state().is_inactive() {
            return None;
        }

        let snap = self.snapshot();
        self.reason = Default::default();

        Some(Self::run_compile(
            handler.clone(),
            snap,
            self.export_target,
            self.syntax_only,
        ))
    }

    /// Compile the document once.
    fn run_compile(
        h: Arc<dyn CompileHandler<F, Ext>>,
        graph: Arc<WorldComputeGraph<F>>,
        export_target: ExportTarget,
        syntax_only: bool,
    ) -> impl FnOnce() -> CompiledArtifact<F> {
        let start = tinymist_std::time::Instant::now();

        // todo unwrap main id
        let id = graph.world().main_id().unwrap();
        let revision = graph.world().revision().get();

        h.status(revision, {
            CompileReport {
                id: graph.snap.id.clone(),
                compiling_id: Some(id),
                page_count: 0,
                status: CompileStatusEnum::Compiling,
            }
        });

        move || {
            let compiled = if syntax_only {
                let main = graph.snap.world.main();
                let source_res = graph.world().source(main).at(Span::from_range(main, 0..0));
                let syntax_res = source_res.and_then(|source| {
                    let errors = source.root().errors();
                    if errors.is_empty() {
                        Ok(())
                    } else {
                        Err(errors.into_iter().map(|s| s.into()).collect())
                    }
                });
                let diag = Arc::new(DiagnosticsTask::from_errors(syntax_res.err()));

                CompiledArtifact {
                    diag,
                    graph,
                    doc: None,
                    deps: OnceLock::default(),
                }
            } else {
                CompiledArtifact::from_graph(graph, matches!(export_target, ExportTarget::Html))
            };

            let res = CompileStatusResult {
                diag: (compiled.warning_cnt() + compiled.error_cnt()) as u32,
                elapsed: start.elapsed(),
            };
            let rep = CompileReport {
                id: compiled.id().clone(),
                compiling_id: Some(id),
                page_count: compiled.doc.as_ref().map_or(0, |doc| doc.num_of_pages()),
                status: match &compiled.doc {
                    Some(..) => CompileStatusEnum::CompileSuccess(res),
                    None if res.diag == 0 => CompileStatusEnum::CompileSuccess(res),
                    None => CompileStatusEnum::CompileError(res),
                },
            };

            // todo: we need to check revision for really concurrent compilation
            log_compile_report(&rep);

            if compiled
                .diagnostics()
                .any(|d| d.message == FILE_MISSING_ERROR_MSG)
            {
                return compiled;
            }

            h.status(revision, rep);
            h.notify_compile(&compiled);
            compiled
        }
    }

    fn process_compile(&mut self, artifact: CompiledArtifact<F>) -> bool {
        let world = &artifact.snap.world;
        let compiled_revision = world.revision().get();
        if self.committed_revision >= compiled_revision {
            return false;
        }

        // Updates state.
        let doc = artifact.doc.clone();
        self.committed_revision = compiled_revision;
        if doc.is_some() {
            self.latest_success_doc = doc;
        }
        self.cached_snapshot = None; // invalidate; will be recomputed on demand

        // Notifies the new file dependencies.
        let mut deps = eco_vec![];
        world.iter_dependencies(&mut |dep| {
            if let Ok(x) = world.file_path(dep).and_then(|e| e.to_err()) {
                deps.push(x.into())
            }
        });

        self.deps = deps.clone();

        let mut world = world.clone();

        let is_primary = self.id == ProjectInsId("primary".into());

        // Trigger an evict task.
        spawn_cpu(move || {
            let evict_start = tinymist_std::time::Instant::now();
            if is_primary {
                comemo::evict(10);

                // Since all the projects share the same cache, we need to evict the cache
                // on the primary instance for all the projects.
                world.evict_source_cache(30);
            }
            world.evict_vfs(60);
            let elapsed = evict_start.elapsed();
            log::debug!("ProjectCompiler: evict cache in {elapsed:?}");
        });

        true
    }
}

fn log_compile_report(rep: &CompileReport) {
    log::info!("{}", rep.message());
}

#[inline]
fn log_send_error<T>(chan: &'static str, res: Result<(), mpsc::error::SendError<T>>) -> bool {
    res.map_err(|err| log::warn!("ProjectCompiler: send to {chan} error: {err}"))
        .is_ok()
}

#[derive(Debug, Clone, Default)]
struct ProjectDeps {
    project_deps: rpds::RedBlackTreeMapSync<ProjectInsId, EcoVec<ImmutPath>>,
}

impl NotifyDeps for ProjectDeps {
    fn dependencies(&self, f: &mut dyn FnMut(&ImmutPath)) {
        for deps in self.project_deps.values().flat_map(|e| e.iter()) {
            f(deps);
        }
    }
}

// todo: move me to tinymist-std
#[cfg(not(target_arch = "wasm32"))]
/// Spawns a CPU thread to run a computing-heavy task.
pub fn spawn_cpu<F>(func: F)
where
    F: FnOnce() + Send + 'static,
{
    rayon::spawn(func);
}

#[cfg(target_arch = "wasm32")]
/// Spawns a CPU thread to run a computing-heavy task.
pub fn spawn_cpu<F>(func: F)
where
    F: FnOnce() + Send + 'static,
{
    func();
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use tinymist_world::{
        mock::{MockCompilerFeat, MockWorkspaceWorldExt},
        vfs::{
            FileChangeSet, FileSnapshot, FilesystemEvent,
            mock::{MockChange, MockWorkspace},
        },
    };
    use tokio::sync::mpsc;
    use typst::{
        diag::{FileError, FileResult},
        foundations::Bytes,
    };

    use crate::mock::{MockProjectBuilderExt, MockProjectChangeExt, MockProjectCompiler};

    const MAIN: &str = "main.typ";
    const DEP: &str = "dep.typ";
    const RENAMED_DEP: &str = "renamed.typ";
    const UNRELATED: &str = "notes.typ";

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum MatrixOperation {
        InitialSync,
        FollowUpNonSyncUpdate,
        CreateDependency,
        EditEntry,
        EditDependency,
        CreateUnrelated,
        RemoveDependency,
        ReadErrorDependency,
        EmptyDependency,
        EmptyUnrelated,
        RenameUpdatedReferences,
        RenameStaleReferences,
        DeleteThenRecreate,
        FailedReadThenRecovery,
        RenameBatch,
        MultiFileUnrelatedBatch,
        UpstreamInvalidation,
        UnrelatedChurn,
        EmptyChangeset,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum EventVariant {
        Update,
        UpstreamUpdate,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SyncMode {
        Sync,
        NonSync,
        NotApplicable,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum InsertPayload {
        NonEmptyContent,
        EmptyContent,
        ReadErrorSnapshot,
        NoInserts,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum RemovePayload {
        NoRemoves,
        OneRemovedPath,
        MultipleRemovedPaths,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum PathRelation {
        EntryFile,
        ImportedDependency,
        PreviouslyDependedPath,
        NewlyCreatedDependency,
        NewlyReferencedDependency,
        UnrelatedFile,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum BatchShape {
        InsertOnly,
        RemoveOnly,
        RemovePlusInsert,
        MultiFileBatch,
        EmptyChangeset,
        RemoveOnlyThenInsertOnly,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum SequenceShape {
        InitialSync,
        OneStepEdit,
        CreateAfterMissingImport,
        OneStepRemove,
        RenameOldPlusNew,
        FailedRead,
        FailedReadThenRecovery,
        TransientEmptyWrite,
        DeleteThenRecreate,
        DelayedMemoryThenFilesystem,
        EmptyChangeset,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ExpectedOutcome {
        IgnoredFirstSync,
        FsReasonRefreshesDependency,
        RecoversNewDependency,
        RefreshesEntrySource,
        RefreshesDependencySource,
        KeepsUnrelatedCreateHarmless,
        ReportsRetiredDependencyUnavailable,
        SurfacesReadErrorDiagnostics,
        UsesEmptyDependencySnapshot,
        KeepsEmptyUnrelatedHarmless,
        FollowsRenamedPath,
        ReportsOldImportUnavailable,
        ReportsThenRecoversRecreatedSource,
        ClearsDiagnosticsAfterRecovery,
        RenameBatchFollowsRenamedPath,
        MultiFileUnrelatedBatchHarmless,
        AppliesDelayedMemoryBeforeFilesystem,
        KeepsUnrelatedChurnHarmless,
        ExplicitNoContentOutcome,
    }

    #[derive(Debug, Clone, Copy)]
    struct MatrixRow {
        operation: MatrixOperation,
        event_variant: EventVariant,
        sync_mode: SyncMode,
        insert_payload: InsertPayload,
        remove_payload: RemovePayload,
        path_relations: &'static [PathRelation],
        batch_shape: BatchShape,
        sequence_shape: SequenceShape,
        expected: ExpectedOutcome,
    }

    impl MatrixRow {
        fn sync_bool(self) -> bool {
            match self.sync_mode {
                SyncMode::Sync => true,
                SyncMode::NonSync => false,
                SyncMode::NotApplicable => {
                    panic!(
                        "matrix row {:?} does not carry an update sync flag",
                        self.operation
                    )
                }
            }
        }

        fn apply_update(self, harness: &mut ProjectCompilerHarness, change: &MockChange) {
            assert_eq!(self.event_variant, EventVariant::Update);
            harness.apply_update(change, self.sync_bool());
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum OmittedEventCombination {
        SyncFlagOnUpstreamUpdate,
        EntryFileReadErrorAfterDirectClientInput,
        BackendSpecificNotifyRenameQuirk,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum OmissionReason {
        Unreachable,
        Redundant,
        Deferred,
    }

    #[derive(Debug)]
    struct OmittedCombination {
        combination: OmittedEventCombination,
        reason: OmissionReason,
    }

    const PROJECT_COMPILER_FS_EVENT_MATRIX: &[MatrixRow] = &[
        MatrixRow {
            operation: MatrixOperation::InitialSync,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::Sync,
            insert_payload: InsertPayload::NonEmptyContent,
            remove_payload: RemovePayload::NoRemoves,
            path_relations: &[PathRelation::EntryFile, PathRelation::ImportedDependency],
            batch_shape: BatchShape::MultiFileBatch,
            sequence_shape: SequenceShape::InitialSync,
            expected: ExpectedOutcome::IgnoredFirstSync,
        },
        MatrixRow {
            operation: MatrixOperation::FollowUpNonSyncUpdate,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::NonEmptyContent,
            remove_payload: RemovePayload::NoRemoves,
            path_relations: &[PathRelation::ImportedDependency],
            batch_shape: BatchShape::InsertOnly,
            sequence_shape: SequenceShape::OneStepEdit,
            expected: ExpectedOutcome::FsReasonRefreshesDependency,
        },
        MatrixRow {
            operation: MatrixOperation::CreateDependency,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::NonEmptyContent,
            remove_payload: RemovePayload::NoRemoves,
            path_relations: &[PathRelation::NewlyCreatedDependency],
            batch_shape: BatchShape::InsertOnly,
            sequence_shape: SequenceShape::CreateAfterMissingImport,
            expected: ExpectedOutcome::RecoversNewDependency,
        },
        MatrixRow {
            operation: MatrixOperation::EditEntry,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::NonEmptyContent,
            remove_payload: RemovePayload::NoRemoves,
            path_relations: &[PathRelation::EntryFile],
            batch_shape: BatchShape::InsertOnly,
            sequence_shape: SequenceShape::OneStepEdit,
            expected: ExpectedOutcome::RefreshesEntrySource,
        },
        MatrixRow {
            operation: MatrixOperation::EditDependency,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::NonEmptyContent,
            remove_payload: RemovePayload::NoRemoves,
            path_relations: &[PathRelation::ImportedDependency],
            batch_shape: BatchShape::InsertOnly,
            sequence_shape: SequenceShape::OneStepEdit,
            expected: ExpectedOutcome::RefreshesDependencySource,
        },
        MatrixRow {
            operation: MatrixOperation::CreateUnrelated,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::NonEmptyContent,
            remove_payload: RemovePayload::NoRemoves,
            path_relations: &[PathRelation::UnrelatedFile],
            batch_shape: BatchShape::InsertOnly,
            sequence_shape: SequenceShape::OneStepEdit,
            expected: ExpectedOutcome::KeepsUnrelatedCreateHarmless,
        },
        MatrixRow {
            operation: MatrixOperation::RemoveDependency,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::NoInserts,
            remove_payload: RemovePayload::OneRemovedPath,
            path_relations: &[PathRelation::PreviouslyDependedPath],
            batch_shape: BatchShape::RemoveOnly,
            sequence_shape: SequenceShape::OneStepRemove,
            expected: ExpectedOutcome::ReportsRetiredDependencyUnavailable,
        },
        MatrixRow {
            operation: MatrixOperation::ReadErrorDependency,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::ReadErrorSnapshot,
            remove_payload: RemovePayload::NoRemoves,
            path_relations: &[PathRelation::ImportedDependency],
            batch_shape: BatchShape::InsertOnly,
            sequence_shape: SequenceShape::FailedRead,
            expected: ExpectedOutcome::SurfacesReadErrorDiagnostics,
        },
        MatrixRow {
            operation: MatrixOperation::EmptyDependency,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::EmptyContent,
            remove_payload: RemovePayload::NoRemoves,
            path_relations: &[PathRelation::ImportedDependency],
            batch_shape: BatchShape::InsertOnly,
            sequence_shape: SequenceShape::TransientEmptyWrite,
            expected: ExpectedOutcome::UsesEmptyDependencySnapshot,
        },
        MatrixRow {
            operation: MatrixOperation::EmptyUnrelated,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::EmptyContent,
            remove_payload: RemovePayload::NoRemoves,
            path_relations: &[PathRelation::UnrelatedFile],
            batch_shape: BatchShape::InsertOnly,
            sequence_shape: SequenceShape::TransientEmptyWrite,
            expected: ExpectedOutcome::KeepsEmptyUnrelatedHarmless,
        },
        MatrixRow {
            operation: MatrixOperation::RenameUpdatedReferences,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::NonEmptyContent,
            remove_payload: RemovePayload::OneRemovedPath,
            path_relations: &[
                PathRelation::PreviouslyDependedPath,
                PathRelation::NewlyReferencedDependency,
            ],
            batch_shape: BatchShape::RemovePlusInsert,
            sequence_shape: SequenceShape::RenameOldPlusNew,
            expected: ExpectedOutcome::FollowsRenamedPath,
        },
        MatrixRow {
            operation: MatrixOperation::RenameStaleReferences,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::NonEmptyContent,
            remove_payload: RemovePayload::OneRemovedPath,
            path_relations: &[PathRelation::PreviouslyDependedPath],
            batch_shape: BatchShape::RemovePlusInsert,
            sequence_shape: SequenceShape::RenameOldPlusNew,
            expected: ExpectedOutcome::ReportsOldImportUnavailable,
        },
        MatrixRow {
            operation: MatrixOperation::DeleteThenRecreate,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::NonEmptyContent,
            remove_payload: RemovePayload::OneRemovedPath,
            path_relations: &[PathRelation::PreviouslyDependedPath],
            batch_shape: BatchShape::RemoveOnlyThenInsertOnly,
            sequence_shape: SequenceShape::DeleteThenRecreate,
            expected: ExpectedOutcome::ReportsThenRecoversRecreatedSource,
        },
        MatrixRow {
            operation: MatrixOperation::FailedReadThenRecovery,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::NonEmptyContent,
            remove_payload: RemovePayload::NoRemoves,
            path_relations: &[PathRelation::ImportedDependency],
            batch_shape: BatchShape::InsertOnly,
            sequence_shape: SequenceShape::FailedReadThenRecovery,
            expected: ExpectedOutcome::ClearsDiagnosticsAfterRecovery,
        },
        MatrixRow {
            operation: MatrixOperation::RenameBatch,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::NonEmptyContent,
            remove_payload: RemovePayload::OneRemovedPath,
            path_relations: &[
                PathRelation::PreviouslyDependedPath,
                PathRelation::NewlyReferencedDependency,
            ],
            batch_shape: BatchShape::RemovePlusInsert,
            sequence_shape: SequenceShape::RenameOldPlusNew,
            expected: ExpectedOutcome::RenameBatchFollowsRenamedPath,
        },
        MatrixRow {
            operation: MatrixOperation::MultiFileUnrelatedBatch,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::NonEmptyContent,
            remove_payload: RemovePayload::MultipleRemovedPaths,
            path_relations: &[PathRelation::UnrelatedFile],
            batch_shape: BatchShape::MultiFileBatch,
            sequence_shape: SequenceShape::OneStepEdit,
            expected: ExpectedOutcome::MultiFileUnrelatedBatchHarmless,
        },
        MatrixRow {
            operation: MatrixOperation::UpstreamInvalidation,
            event_variant: EventVariant::UpstreamUpdate,
            sync_mode: SyncMode::NotApplicable,
            insert_payload: InsertPayload::NonEmptyContent,
            remove_payload: RemovePayload::NoRemoves,
            path_relations: &[PathRelation::EntryFile],
            batch_shape: BatchShape::InsertOnly,
            sequence_shape: SequenceShape::DelayedMemoryThenFilesystem,
            expected: ExpectedOutcome::AppliesDelayedMemoryBeforeFilesystem,
        },
        MatrixRow {
            operation: MatrixOperation::UnrelatedChurn,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::NonEmptyContent,
            remove_payload: RemovePayload::NoRemoves,
            path_relations: &[PathRelation::UnrelatedFile],
            batch_shape: BatchShape::InsertOnly,
            sequence_shape: SequenceShape::OneStepEdit,
            expected: ExpectedOutcome::KeepsUnrelatedChurnHarmless,
        },
        MatrixRow {
            operation: MatrixOperation::EmptyChangeset,
            event_variant: EventVariant::Update,
            sync_mode: SyncMode::NonSync,
            insert_payload: InsertPayload::NoInserts,
            remove_payload: RemovePayload::NoRemoves,
            path_relations: &[PathRelation::UnrelatedFile],
            batch_shape: BatchShape::EmptyChangeset,
            sequence_shape: SequenceShape::EmptyChangeset,
            expected: ExpectedOutcome::ExplicitNoContentOutcome,
        },
    ];

    const OMITTED_PROJECT_COMPILER_FS_EVENT_COMBINATIONS: &[OmittedCombination] = &[
        OmittedCombination {
            combination: OmittedEventCombination::SyncFlagOnUpstreamUpdate,
            reason: OmissionReason::Unreachable,
        },
        OmittedCombination {
            combination: OmittedEventCombination::EntryFileReadErrorAfterDirectClientInput,
            reason: OmissionReason::Redundant,
        },
        OmittedCombination {
            combination: OmittedEventCombination::BackendSpecificNotifyRenameQuirk,
            reason: OmissionReason::Deferred,
        },
    ];

    struct ProjectCompilerHarness {
        workspace: MockWorkspace,
        compiler: MockProjectCompiler<()>,
        notify_rx: mpsc::UnboundedReceiver<NotifyMessage>,
    }

    impl ProjectCompilerHarness {
        fn new(files: &[(&str, &str)]) -> Self {
            Self::with_opts(
                files,
                CompileServerOpts::<MockCompilerFeat, ()> {
                    syntax_only: false,
                    ..Default::default()
                },
            )
        }

        fn ignoring_first_sync(files: &[(&str, &str)]) -> Self {
            Self::with_opts(
                files,
                CompileServerOpts::<MockCompilerFeat, ()> {
                    ignore_first_sync: true,
                    syntax_only: false,
                    ..Default::default()
                },
            )
        }

        fn with_opts(
            files: &[(&str, &str)],
            opts: CompileServerOpts<MockCompilerFeat, ()>,
        ) -> Self {
            let mut builder = MockWorkspace::default_builder();
            for (path, source) in files {
                builder = builder.file(path, source.to_string());
            }

            let workspace = builder.build();
            let (compiler, notify_rx) = workspace
                .world(MAIN)
                .project_compiler_with_opts::<()>(opts)
                .unwrap();

            Self {
                workspace,
                compiler,
                notify_rx,
            }
        }

        fn compile_primary(&mut self) -> CompiledArtifact<MockCompilerFeat> {
            self.compiler
                .process(Interrupt::Compile(ProjectInsId::PRIMARY));
            self.compile_pending()
        }

        fn compile_pending(&mut self) -> CompiledArtifact<MockCompilerFeat> {
            assert!(
                self.compiler.primary.reason.any(),
                "expected a pending compile reason"
            );

            let handler = self.compiler.handler.clone();
            let compile = self
                .compiler
                .primary
                .may_compile(&handler)
                .expect("expected the primary project to compile");
            let artifact = compile();
            self.compiler.process(Interrupt::Compiled(artifact.clone()));
            artifact
        }

        fn apply_update(&mut self, change: &MockChange, is_sync: bool) {
            change.apply_as_fs_to_project(&mut self.compiler, is_sync);
        }

        fn apply_upstream_update(
            &mut self,
            changeset: FileChangeSet,
            upstream_event: Option<UpstreamUpdateEvent>,
        ) {
            self.compiler
                .process(Interrupt::Fs(FilesystemEvent::UpstreamUpdate {
                    changeset,
                    upstream_event,
                }));
        }

        fn take_upstream_update(&mut self) -> UpstreamUpdateEvent {
            loop {
                let message = self
                    .notify_rx
                    .try_recv()
                    .expect("expected an upstream update notification");
                match message {
                    NotifyMessage::UpstreamUpdate(event) => return event,
                    NotifyMessage::SyncDependency(..) | NotifyMessage::Settle => {}
                }
            }
        }

        fn latest_sync_dependencies(&mut self) -> Vec<PathBuf> {
            self.optional_sync_dependencies()
                .expect("expected SyncDependency notification")
        }

        fn optional_sync_dependencies(&mut self) -> Option<Vec<PathBuf>> {
            let mut latest = None;
            while let Ok(message) = self.notify_rx.try_recv() {
                if let NotifyMessage::SyncDependency(deps) = message {
                    let mut paths = Vec::new();
                    deps.dependencies(&mut |path| paths.push(path.as_ref().to_path_buf()));
                    latest = Some(paths);
                }
            }

            latest
        }

        fn dependency_paths_after_compile(&mut self) -> Vec<PathBuf> {
            self.latest_sync_dependencies()
        }

        fn dependency_paths_after_harmless_compile(
            &mut self,
            previous: &[PathBuf],
        ) -> Vec<PathBuf> {
            self.optional_sync_dependencies()
                .unwrap_or_else(|| previous.to_vec())
        }
    }

    fn default_files() -> Vec<(&'static str, &'static str)> {
        vec![
            (MAIN, "#import \"dep.typ\": value\n#value"),
            (DEP, "#let value = [before]"),
            (UNRELATED, "#let note = [unchanged]"),
        ]
    }

    fn source_snapshot(source: &str) -> FileSnapshot {
        FileResult::Ok(Bytes::from_string(source.to_owned())).into()
    }

    fn read_error_snapshot(path: PathBuf) -> FileSnapshot {
        FileResult::Err(FileError::NotFound(path)).into()
    }

    fn insert_source_change(workspace: &MockWorkspace, path: &str, source: &str) -> MockChange {
        MockChange::new(FileChangeSet::new_inserts(vec![(
            workspace.immut_path(path),
            source_snapshot(source),
        )]))
    }

    fn read_error_change(workspace: &MockWorkspace, path: &str) -> MockChange {
        MockChange::new(FileChangeSet::new_inserts(vec![(
            workspace.immut_path(path),
            read_error_snapshot(workspace.path(path)),
        )]))
    }

    fn remove_change(workspace: &MockWorkspace, path: &str) -> MockChange {
        MockChange::new(FileChangeSet::new_removes(vec![workspace.immut_path(path)]))
    }

    fn empty_change() -> MockChange {
        MockChange::new(FileChangeSet::default())
    }

    fn combine_changes(changes: &[MockChange]) -> MockChange {
        let mut changeset = FileChangeSet::default();
        for change in changes {
            changeset.removes.extend(change.changeset().removes.clone());
            changeset.inserts.extend(change.changeset().inserts.clone());
        }

        MockChange::new(changeset)
    }

    fn source_text(
        artifact: &CompiledArtifact<MockCompilerFeat>,
        workspace: &MockWorkspace,
        path: &str,
    ) -> String {
        artifact
            .graph
            .snap
            .world
            .source_by_path(&workspace.path(path))
            .unwrap()
            .text()
            .to_owned()
    }

    fn source_is_unavailable(
        artifact: &CompiledArtifact<MockCompilerFeat>,
        workspace: &MockWorkspace,
        path: &str,
    ) -> bool {
        artifact
            .graph
            .snap
            .world
            .source_by_path(&workspace.path(path))
            .is_err()
    }

    fn assert_fs_reason(compiler: &MockProjectCompiler<()>) {
        assert!(
            compiler.primary.reason.by_fs_events,
            "expected filesystem compile reason"
        );
    }

    fn assert_mem_reason(compiler: &MockProjectCompiler<()>) {
        assert!(
            compiler.primary.reason.by_mem_events,
            "expected memory compile reason"
        );
    }

    fn assert_deps_contain(workspace: &MockWorkspace, deps: &[PathBuf], path: &str) {
        assert!(
            deps.contains(&workspace.path(path)),
            "expected dependencies to contain {path:?}; got {deps:?}"
        );
    }

    fn assert_deps_do_not_contain(workspace: &MockWorkspace, deps: &[PathBuf], path: &str) {
        assert!(
            !deps.contains(&workspace.path(path)),
            "expected dependencies not to contain {path:?}; got {deps:?}"
        );
    }

    fn assert_matrix_contains<T: std::fmt::Debug>(
        missing: T,
        predicate: impl Fn(&MatrixRow) -> bool,
    ) {
        assert!(
            PROJECT_COMPILER_FS_EVENT_MATRIX.iter().any(predicate),
            "project compiler filesystem event matrix missing {missing:?}"
        );
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "project compiler matrix rows are clearer when each dimension is asserted explicitly"
    )]
    fn assert_row_shape(
        row: MatrixRow,
        event_variant: EventVariant,
        sync_mode: SyncMode,
        insert_payload: InsertPayload,
        remove_payload: RemovePayload,
        path_relations: &[PathRelation],
        batch_shape: BatchShape,
        sequence_shape: SequenceShape,
        expected: ExpectedOutcome,
    ) {
        assert_eq!(row.event_variant, event_variant);
        assert_eq!(row.sync_mode, sync_mode);
        assert_eq!(row.insert_payload, insert_payload);
        assert_eq!(row.remove_payload, remove_payload);
        assert_eq!(row.path_relations, path_relations);
        assert_eq!(row.batch_shape, batch_shape);
        assert_eq!(row.sequence_shape, sequence_shape);
        assert_eq!(row.expected, expected);
    }

    fn clean_default_harness_with_deps() -> (ProjectCompilerHarness, Vec<PathBuf>) {
        let files = default_files();
        let mut harness = ProjectCompilerHarness::new(&files);
        let initial = harness.compile_primary();
        assert_eq!(initial.error_cnt(), 0);
        let deps = harness.latest_sync_dependencies();
        (harness, deps)
    }

    fn clean_default_harness() -> ProjectCompilerHarness {
        clean_default_harness_with_deps().0
    }

    fn run_matrix_row(row: MatrixRow) {
        match row.operation {
            MatrixOperation::InitialSync => assert_initial_sync(row),
            MatrixOperation::FollowUpNonSyncUpdate => assert_follow_up_non_sync_update(row),
            MatrixOperation::CreateDependency => assert_create_dependency(row),
            MatrixOperation::EditEntry => assert_edit_entry(row),
            MatrixOperation::EditDependency => assert_edit_dependency(row),
            MatrixOperation::CreateUnrelated => assert_create_unrelated(row),
            MatrixOperation::RemoveDependency => assert_remove_dependency(row),
            MatrixOperation::ReadErrorDependency => assert_read_error_dependency(row),
            MatrixOperation::EmptyDependency => assert_empty_dependency(row),
            MatrixOperation::EmptyUnrelated => assert_empty_unrelated(row),
            MatrixOperation::RenameUpdatedReferences => assert_rename_updated_references(row),
            MatrixOperation::RenameStaleReferences => assert_rename_stale_references(row),
            MatrixOperation::DeleteThenRecreate => assert_delete_then_recreate(row),
            MatrixOperation::FailedReadThenRecovery => assert_failed_read_then_recovery(row),
            MatrixOperation::RenameBatch => assert_rename_batch(row),
            MatrixOperation::MultiFileUnrelatedBatch => assert_multi_file_unrelated_batch(row),
            MatrixOperation::UpstreamInvalidation => assert_upstream_invalidation(row),
            MatrixOperation::UnrelatedChurn => assert_unrelated_churn(row),
            MatrixOperation::EmptyChangeset => assert_empty_changeset(row),
        }
    }

    #[test]
    fn project_compiler_fs_event_matrix_is_explicit() {
        for row in PROJECT_COMPILER_FS_EVENT_MATRIX {
            assert!(!row.path_relations.is_empty());
        }

        for operation in [
            MatrixOperation::InitialSync,
            MatrixOperation::FollowUpNonSyncUpdate,
            MatrixOperation::CreateDependency,
            MatrixOperation::EditEntry,
            MatrixOperation::EditDependency,
            MatrixOperation::CreateUnrelated,
            MatrixOperation::RemoveDependency,
            MatrixOperation::ReadErrorDependency,
            MatrixOperation::EmptyDependency,
            MatrixOperation::EmptyUnrelated,
            MatrixOperation::RenameUpdatedReferences,
            MatrixOperation::RenameStaleReferences,
            MatrixOperation::DeleteThenRecreate,
            MatrixOperation::FailedReadThenRecovery,
            MatrixOperation::RenameBatch,
            MatrixOperation::MultiFileUnrelatedBatch,
            MatrixOperation::UpstreamInvalidation,
            MatrixOperation::UnrelatedChurn,
            MatrixOperation::EmptyChangeset,
        ] {
            assert_matrix_contains(operation, |row| row.operation == operation);
        }
        for variant in [EventVariant::Update, EventVariant::UpstreamUpdate] {
            assert_matrix_contains(variant, |row| row.event_variant == variant);
        }
        for sync_mode in [SyncMode::Sync, SyncMode::NonSync, SyncMode::NotApplicable] {
            assert_matrix_contains(sync_mode, |row| row.sync_mode == sync_mode);
        }
        for payload in [
            InsertPayload::NonEmptyContent,
            InsertPayload::EmptyContent,
            InsertPayload::ReadErrorSnapshot,
            InsertPayload::NoInserts,
        ] {
            assert_matrix_contains(payload, |row| row.insert_payload == payload);
        }
        for payload in [
            RemovePayload::NoRemoves,
            RemovePayload::OneRemovedPath,
            RemovePayload::MultipleRemovedPaths,
        ] {
            assert_matrix_contains(payload, |row| row.remove_payload == payload);
        }
        for relation in [
            PathRelation::EntryFile,
            PathRelation::ImportedDependency,
            PathRelation::PreviouslyDependedPath,
            PathRelation::NewlyCreatedDependency,
            PathRelation::NewlyReferencedDependency,
            PathRelation::UnrelatedFile,
        ] {
            assert_matrix_contains(relation, |row| row.path_relations.contains(&relation));
        }
        for batch in [
            BatchShape::InsertOnly,
            BatchShape::RemoveOnly,
            BatchShape::RemovePlusInsert,
            BatchShape::MultiFileBatch,
            BatchShape::EmptyChangeset,
            BatchShape::RemoveOnlyThenInsertOnly,
        ] {
            assert_matrix_contains(batch, |row| row.batch_shape == batch);
        }
        for sequence in [
            SequenceShape::InitialSync,
            SequenceShape::OneStepEdit,
            SequenceShape::CreateAfterMissingImport,
            SequenceShape::OneStepRemove,
            SequenceShape::RenameOldPlusNew,
            SequenceShape::FailedRead,
            SequenceShape::FailedReadThenRecovery,
            SequenceShape::TransientEmptyWrite,
            SequenceShape::DeleteThenRecreate,
            SequenceShape::DelayedMemoryThenFilesystem,
            SequenceShape::EmptyChangeset,
        ] {
            assert_matrix_contains(sequence, |row| row.sequence_shape == sequence);
        }

        for omitted in OMITTED_PROJECT_COMPILER_FS_EVENT_COMBINATIONS {
            assert!(matches!(
                omitted.reason,
                OmissionReason::Unreachable | OmissionReason::Redundant | OmissionReason::Deferred
            ));
            assert!(matches!(
                omitted.combination,
                OmittedEventCombination::SyncFlagOnUpstreamUpdate
                    | OmittedEventCombination::EntryFileReadErrorAfterDirectClientInput
                    | OmittedEventCombination::BackendSpecificNotifyRenameQuirk
            ));
        }
    }

    #[test]
    fn project_compiler_fs_event_matrix_rows_execute_expected_outcomes() {
        for row in PROJECT_COMPILER_FS_EVENT_MATRIX {
            run_matrix_row(*row);
        }
    }

    fn assert_initial_sync(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::Sync,
            InsertPayload::NonEmptyContent,
            RemovePayload::NoRemoves,
            &[PathRelation::EntryFile, PathRelation::ImportedDependency],
            BatchShape::MultiFileBatch,
            SequenceShape::InitialSync,
            ExpectedOutcome::IgnoredFirstSync,
        );

        let files = default_files();
        let mut harness = ProjectCompilerHarness::ignoring_first_sync(&files);
        let initial = harness.compile_primary();
        assert_eq!(initial.error_cnt(), 0);
        harness.latest_sync_dependencies();

        let sync = MockChange::new(harness.workspace.sync_changeset());
        row.apply_update(&mut harness, &sync);
        assert!(
            !harness.compiler.primary.reason.any(),
            "initial sync should not create a compile reason when ignored"
        );
    }

    fn assert_follow_up_non_sync_update(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::NonEmptyContent,
            RemovePayload::NoRemoves,
            &[PathRelation::ImportedDependency],
            BatchShape::InsertOnly,
            SequenceShape::OneStepEdit,
            ExpectedOutcome::FsReasonRefreshesDependency,
        );

        let files = default_files();
        let mut harness = ProjectCompilerHarness::ignoring_first_sync(&files);
        let initial = harness.compile_primary();
        assert_eq!(initial.error_cnt(), 0);
        harness.latest_sync_dependencies();

        let sync = MockChange::new(harness.workspace.sync_changeset());
        harness.apply_update(&sync, true);
        assert!(!harness.compiler.primary.reason.any());

        let follow_up = harness
            .workspace
            .update_source(DEP, "#let value = [after sync]");
        row.apply_update(&mut harness, &follow_up);
        assert_fs_reason(&harness.compiler);

        let artifact = harness.compile_pending();
        assert_eq!(artifact.error_cnt(), 0);
        assert_eq!(
            source_text(&artifact, &harness.workspace, DEP),
            "#let value = [after sync]"
        );
        let deps = harness.dependency_paths_after_compile();
        assert_deps_contain(&harness.workspace, &deps, DEP);
    }

    fn assert_create_dependency(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::NonEmptyContent,
            RemovePayload::NoRemoves,
            &[PathRelation::NewlyCreatedDependency],
            BatchShape::InsertOnly,
            SequenceShape::CreateAfterMissingImport,
            ExpectedOutcome::RecoversNewDependency,
        );

        let files = vec![
            (
                MAIN,
                "#import \"dep.typ\": value\n#import \"new.typ\": newer\n#value\n#newer",
            ),
            (DEP, "#let value = [before]"),
        ];
        let mut harness = ProjectCompilerHarness::new(&files);
        let initial = harness.compile_primary();
        assert!(initial.error_cnt() > 0);
        harness.latest_sync_dependencies();

        let created_dependency = harness
            .workspace
            .create_source("new.typ", "#let newer = [new dependency]");
        row.apply_update(&mut harness, &created_dependency);
        assert_fs_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert_eq!(artifact.error_cnt(), 0);
        assert_eq!(
            source_text(&artifact, &harness.workspace, "new.typ"),
            "#let newer = [new dependency]"
        );
        let deps = harness.dependency_paths_after_compile();
        assert_deps_contain(&harness.workspace, &deps, "new.typ");
    }

    fn assert_edit_entry(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::NonEmptyContent,
            RemovePayload::NoRemoves,
            &[PathRelation::EntryFile],
            BatchShape::InsertOnly,
            SequenceShape::OneStepEdit,
            ExpectedOutcome::RefreshesEntrySource,
        );

        let mut harness = clean_default_harness();
        let entry_edit = harness.workspace.update_source(
            MAIN,
            "#import \"dep.typ\": value\n#let local = [entry changed]\n#value\n#local",
        );
        row.apply_update(&mut harness, &entry_edit);
        assert_fs_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert_eq!(artifact.error_cnt(), 0);
        assert_eq!(
            source_text(&artifact, &harness.workspace, MAIN),
            "#import \"dep.typ\": value\n#let local = [entry changed]\n#value\n#local"
        );
    }

    fn assert_edit_dependency(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::NonEmptyContent,
            RemovePayload::NoRemoves,
            &[PathRelation::ImportedDependency],
            BatchShape::InsertOnly,
            SequenceShape::OneStepEdit,
            ExpectedOutcome::RefreshesDependencySource,
        );

        let mut harness = clean_default_harness();
        let dependency_edit = harness
            .workspace
            .update_source(DEP, "#let value = [dependency changed]");
        row.apply_update(&mut harness, &dependency_edit);
        assert_fs_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert_eq!(artifact.error_cnt(), 0);
        assert_eq!(
            source_text(&artifact, &harness.workspace, DEP),
            "#let value = [dependency changed]"
        );
    }

    fn assert_create_unrelated(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::NonEmptyContent,
            RemovePayload::NoRemoves,
            &[PathRelation::UnrelatedFile],
            BatchShape::InsertOnly,
            SequenceShape::OneStepEdit,
            ExpectedOutcome::KeepsUnrelatedCreateHarmless,
        );

        let (mut harness, deps_before) = clean_default_harness_with_deps();
        let unrelated_create = harness
            .workspace
            .create_source("scratch.typ", "#let scratch = [unused]");
        row.apply_update(&mut harness, &unrelated_create);
        assert_fs_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert_eq!(artifact.error_cnt(), 0);
        let deps_after = harness.dependency_paths_after_harmless_compile(&deps_before);
        assert_eq!(deps_after, deps_before);
        assert_deps_do_not_contain(&harness.workspace, &deps_after, "scratch.typ");
    }

    fn assert_remove_dependency(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::NoInserts,
            RemovePayload::OneRemovedPath,
            &[PathRelation::PreviouslyDependedPath],
            BatchShape::RemoveOnly,
            SequenceShape::OneStepRemove,
            ExpectedOutcome::ReportsRetiredDependencyUnavailable,
        );

        let mut harness = clean_default_harness();
        let removed = harness.workspace.remove(DEP).unwrap();
        row.apply_update(&mut harness, &removed);
        assert_fs_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert!(artifact.error_cnt() > 0);
        assert!(source_is_unavailable(&artifact, &harness.workspace, DEP));
    }

    fn assert_read_error_dependency(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::ReadErrorSnapshot,
            RemovePayload::NoRemoves,
            &[PathRelation::ImportedDependency],
            BatchShape::InsertOnly,
            SequenceShape::FailedRead,
            ExpectedOutcome::SurfacesReadErrorDiagnostics,
        );

        let mut harness = clean_default_harness();
        let read_error = read_error_change(&harness.workspace, DEP);
        row.apply_update(&mut harness, &read_error);
        assert_fs_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert!(artifact.error_cnt() > 0);
        assert!(source_is_unavailable(&artifact, &harness.workspace, DEP));
    }

    fn assert_empty_dependency(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::EmptyContent,
            RemovePayload::NoRemoves,
            &[PathRelation::ImportedDependency],
            BatchShape::InsertOnly,
            SequenceShape::TransientEmptyWrite,
            ExpectedOutcome::UsesEmptyDependencySnapshot,
        );

        let mut harness = clean_default_harness();
        let empty_dependency = harness.workspace.update_source(DEP, "");
        row.apply_update(&mut harness, &empty_dependency);
        assert_fs_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert!(artifact.error_cnt() > 0);
        assert_eq!(source_text(&artifact, &harness.workspace, DEP), "");
    }

    fn assert_empty_unrelated(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::EmptyContent,
            RemovePayload::NoRemoves,
            &[PathRelation::UnrelatedFile],
            BatchShape::InsertOnly,
            SequenceShape::TransientEmptyWrite,
            ExpectedOutcome::KeepsEmptyUnrelatedHarmless,
        );

        let (mut harness, deps_before) = clean_default_harness_with_deps();
        let empty_unrelated = harness.workspace.update_source(UNRELATED, "");
        row.apply_update(&mut harness, &empty_unrelated);
        assert_fs_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert_eq!(artifact.error_cnt(), 0);
        let deps_after = harness.dependency_paths_after_harmless_compile(&deps_before);
        assert_eq!(deps_after, deps_before);
        assert_deps_do_not_contain(&harness.workspace, &deps_after, UNRELATED);
    }

    fn assert_rename_updated_references(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::NonEmptyContent,
            RemovePayload::OneRemovedPath,
            &[
                PathRelation::PreviouslyDependedPath,
                PathRelation::NewlyReferencedDependency,
            ],
            BatchShape::RemovePlusInsert,
            SequenceShape::RenameOldPlusNew,
            ExpectedOutcome::FollowsRenamedPath,
        );

        let mut harness = clean_default_harness();
        let rename = harness.workspace.rename(DEP, RENAMED_DEP).unwrap();
        row.apply_update(&mut harness, &rename);
        let entry_update = harness
            .workspace
            .update_source(MAIN, "#import \"renamed.typ\": value\n#value");
        harness.apply_update(&entry_update, false);
        assert_fs_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert_eq!(artifact.error_cnt(), 0);
        assert!(source_is_unavailable(&artifact, &harness.workspace, DEP));
        assert_eq!(
            source_text(&artifact, &harness.workspace, RENAMED_DEP),
            "#let value = [before]"
        );
        let deps = harness.dependency_paths_after_compile();
        assert_deps_contain(&harness.workspace, &deps, RENAMED_DEP);
        assert_deps_do_not_contain(&harness.workspace, &deps, DEP);
    }

    fn assert_rename_stale_references(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::NonEmptyContent,
            RemovePayload::OneRemovedPath,
            &[PathRelation::PreviouslyDependedPath],
            BatchShape::RemovePlusInsert,
            SequenceShape::RenameOldPlusNew,
            ExpectedOutcome::ReportsOldImportUnavailable,
        );

        let mut harness = clean_default_harness();
        let rename = harness.workspace.rename(DEP, RENAMED_DEP).unwrap();
        row.apply_update(&mut harness, &rename);
        assert_fs_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert!(artifact.error_cnt() > 0);
        assert!(source_is_unavailable(&artifact, &harness.workspace, DEP));
        assert_eq!(
            source_text(&artifact, &harness.workspace, RENAMED_DEP),
            "#let value = [before]"
        );
    }

    fn assert_delete_then_recreate(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::NonEmptyContent,
            RemovePayload::OneRemovedPath,
            &[PathRelation::PreviouslyDependedPath],
            BatchShape::RemoveOnlyThenInsertOnly,
            SequenceShape::DeleteThenRecreate,
            ExpectedOutcome::ReportsThenRecoversRecreatedSource,
        );

        let mut harness = clean_default_harness();
        let removed = harness.workspace.remove(DEP).unwrap();
        row.apply_update(&mut harness, &removed);
        let artifact = harness.compile_pending();
        assert!(artifact.error_cnt() > 0);
        assert!(source_is_unavailable(&artifact, &harness.workspace, DEP));
        harness.latest_sync_dependencies();

        let recreated = harness
            .workspace
            .create_source(DEP, "#let value = [recreated]");
        row.apply_update(&mut harness, &recreated);
        assert_fs_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert_eq!(artifact.error_cnt(), 0);
        assert_eq!(
            source_text(&artifact, &harness.workspace, DEP),
            "#let value = [recreated]"
        );
        let deps = harness.dependency_paths_after_compile();
        assert_deps_contain(&harness.workspace, &deps, DEP);
    }

    fn assert_failed_read_then_recovery(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::NonEmptyContent,
            RemovePayload::NoRemoves,
            &[PathRelation::ImportedDependency],
            BatchShape::InsertOnly,
            SequenceShape::FailedReadThenRecovery,
            ExpectedOutcome::ClearsDiagnosticsAfterRecovery,
        );

        let mut harness = clean_default_harness();
        let read_error = read_error_change(&harness.workspace, DEP);
        row.apply_update(&mut harness, &read_error);
        let artifact = harness.compile_pending();
        assert!(artifact.error_cnt() > 0);
        assert!(source_is_unavailable(&artifact, &harness.workspace, DEP));
        harness.latest_sync_dependencies();

        let recovered = harness
            .workspace
            .update_source(DEP, "#let value = [recovered]");
        row.apply_update(&mut harness, &recovered);
        assert_fs_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert_eq!(artifact.error_cnt(), 0);
        assert_eq!(
            source_text(&artifact, &harness.workspace, DEP),
            "#let value = [recovered]"
        );
        let deps = harness.dependency_paths_after_compile();
        assert_deps_contain(&harness.workspace, &deps, DEP);
    }

    fn assert_rename_batch(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::NonEmptyContent,
            RemovePayload::OneRemovedPath,
            &[
                PathRelation::PreviouslyDependedPath,
                PathRelation::NewlyReferencedDependency,
            ],
            BatchShape::RemovePlusInsert,
            SequenceShape::RenameOldPlusNew,
            ExpectedOutcome::RenameBatchFollowsRenamedPath,
        );

        let mut harness = clean_default_harness();
        let rename = harness.workspace.rename(DEP, RENAMED_DEP).unwrap();
        let entry_update = harness
            .workspace
            .update_source(MAIN, "#import \"renamed.typ\": value\n#value");
        let batch = combine_changes(&[rename, entry_update]);
        row.apply_update(&mut harness, &batch);
        assert_fs_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert_eq!(artifact.error_cnt(), 0);
        let deps = harness.dependency_paths_after_compile();
        assert_deps_contain(&harness.workspace, &deps, RENAMED_DEP);
        assert_deps_do_not_contain(&harness.workspace, &deps, DEP);
    }

    fn assert_multi_file_unrelated_batch(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::NonEmptyContent,
            RemovePayload::MultipleRemovedPaths,
            &[PathRelation::UnrelatedFile],
            BatchShape::MultiFileBatch,
            SequenceShape::OneStepEdit,
            ExpectedOutcome::MultiFileUnrelatedBatchHarmless,
        );

        let files = vec![
            (MAIN, "#import \"dep.typ\": value\n#value"),
            (DEP, "#let value = [before]"),
            ("old-a.typ", "#let old_a = [unused]"),
            ("old-b.typ", "#let old_b = [unused]"),
        ];
        let mut harness = ProjectCompilerHarness::new(&files);
        let initial = harness.compile_primary();
        assert_eq!(initial.error_cnt(), 0);
        let deps_before = harness.latest_sync_dependencies();

        let remove_a = harness.workspace.remove("old-a.typ").unwrap();
        let remove_b = harness.workspace.remove("old-b.typ").unwrap();
        let create_a = harness
            .workspace
            .create_source("new-a.typ", "#let new_a = [unused]");
        let create_b = harness
            .workspace
            .create_source("new-b.typ", "#let new_b = [unused]");
        let batch = combine_changes(&[remove_a, remove_b, create_a, create_b]);
        row.apply_update(&mut harness, &batch);
        assert_fs_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert_eq!(artifact.error_cnt(), 0);
        let deps_after = harness.dependency_paths_after_harmless_compile(&deps_before);
        assert_eq!(deps_after, deps_before);
        assert_deps_do_not_contain(&harness.workspace, &deps_after, "new-a.typ");
        assert_deps_do_not_contain(&harness.workspace, &deps_after, "new-b.typ");
    }

    fn assert_upstream_invalidation(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::UpstreamUpdate,
            SyncMode::NotApplicable,
            InsertPayload::NonEmptyContent,
            RemovePayload::NoRemoves,
            &[PathRelation::EntryFile],
            BatchShape::InsertOnly,
            SequenceShape::DelayedMemoryThenFilesystem,
            ExpectedOutcome::AppliesDelayedMemoryBeforeFilesystem,
        );

        let files = vec![(MAIN, "#let value = [disk]\n#value")];
        let mut harness = ProjectCompilerHarness::new(&files);
        let initial = harness.compile_primary();
        assert_eq!(initial.error_cnt(), 0);
        harness.latest_sync_dependencies();

        let memory_insert = insert_source_change(
            &harness.workspace,
            MAIN,
            "#let value = [memory shadow]\n#value",
        );
        harness
            .compiler
            .process(Interrupt::Memory(memory_insert.memory_event()));
        assert_mem_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert_eq!(
            source_text(&artifact, &harness.workspace, MAIN),
            "#let value = [memory shadow]\n#value"
        );
        harness.latest_sync_dependencies();

        let memory_remove = remove_change(&harness.workspace, MAIN);
        harness
            .compiler
            .process(Interrupt::Memory(memory_remove.memory_event()));
        let upstream_event = harness.take_upstream_update();
        assert!(
            upstream_event
                .invalidates
                .contains(&harness.workspace.immut_path(MAIN))
        );
        assert!(
            !harness.compiler.primary.reason.any(),
            "delayed memory removal should wait for the upstream filesystem event"
        );

        let filesystem_update = harness
            .workspace
            .update_source(MAIN, "#let value = [filesystem]\n#value");
        harness.apply_upstream_update(filesystem_update.into_changeset(), Some(upstream_event));
        assert_fs_reason(&harness.compiler);
        assert!(
            !harness.compiler.primary.reason.by_mem_events,
            "known upstream update should not add a separate memory reason"
        );

        let artifact = harness.compile_pending();
        assert_eq!(artifact.error_cnt(), 0);
        assert_eq!(
            source_text(&artifact, &harness.workspace, MAIN),
            "#let value = [filesystem]\n#value"
        );
    }

    fn assert_unrelated_churn(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::NonEmptyContent,
            RemovePayload::NoRemoves,
            &[PathRelation::UnrelatedFile],
            BatchShape::InsertOnly,
            SequenceShape::OneStepEdit,
            ExpectedOutcome::KeepsUnrelatedChurnHarmless,
        );

        let (mut harness, deps_before) = clean_default_harness_with_deps();
        let unrelated = harness
            .workspace
            .update_source(UNRELATED, "#let note = [changed but unused]");
        row.apply_update(&mut harness, &unrelated);
        assert_fs_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert_eq!(artifact.error_cnt(), 0);
        let deps_after = harness.dependency_paths_after_harmless_compile(&deps_before);
        assert_eq!(deps_after, deps_before);
        assert_deps_do_not_contain(&harness.workspace, &deps_after, UNRELATED);
    }

    fn assert_empty_changeset(row: MatrixRow) {
        assert_row_shape(
            row,
            EventVariant::Update,
            SyncMode::NonSync,
            InsertPayload::NoInserts,
            RemovePayload::NoRemoves,
            &[PathRelation::UnrelatedFile],
            BatchShape::EmptyChangeset,
            SequenceShape::EmptyChangeset,
            ExpectedOutcome::ExplicitNoContentOutcome,
        );

        let (mut harness, deps_before) = clean_default_harness_with_deps();
        let empty = empty_change();
        row.apply_update(&mut harness, &empty);
        assert_fs_reason(&harness.compiler);
        let artifact = harness.compile_pending();
        assert_eq!(artifact.error_cnt(), 0);
        let deps_after = harness.dependency_paths_after_harmless_compile(&deps_before);
        assert_eq!(deps_after, deps_before);
    }
}
