//! Project compiler for tinymist.

use core::fmt;
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use ecow::{eco_vec, EcoVec};
use tinymist_std::error::prelude::Result;
use tinymist_std::{typst::TypstDocument, ImmutPath};
use tinymist_task::ExportTarget;
use tinymist_world::vfs::notify::{
    FilesystemEvent, MemoryEvent, NotifyDeps, NotifyMessage, UpstreamUpdateEvent,
};
use tinymist_world::vfs::{FileId, FsProvider, RevisingVfs, WorkspaceResolver};
use tinymist_world::{
    CompileSnapshot, CompilerFeat, CompilerUniverse, DiagnosticsTask, EntryReader, EntryState,
    ExportSignal, FlagTask, HtmlCompilationTask, PagedCompilationTask, ProjectInsId, TaskInputs,
    WorldComputeGraph, WorldDeps,
};
use tokio::sync::mpsc;

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
    pub fn diagnostics(&self) -> impl Iterator<Item = &typst::diag::SourceDiagnostic> {
        self.diag.diagnostics()
    }

    /// Returns whether there are any errors.
    pub fn has_errors(&self) -> bool {
        self.error_cnt() > 0
    }

    /// Sets the signal.
    pub fn with_signal(mut self, signal: ExportSignal) -> Self {
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

#[allow(missing_docs)]
impl CompileReport {
    /// Get the status message.
    pub fn message(&self) -> CompileReportMsg<'_> {
        CompileReportMsg(self)
    }
}

#[allow(missing_docs)]
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
                return write!(f, "{input:?}: compilation succeeded in {elapsed:?}")
            }
            CompileSuccess(res) => ("compilation succeeded", res),
            CompileError(res) => ("compilation failed", res),
            ExportError(res) => ("export failed", res),
        };
        write!(f, "{input:?}: {stage} with {diag} warnings in {elapsed:?}")
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
    /// Memory file changes.
    Memory(MemoryEvent),
    /// File system event.
    Fs(FilesystemEvent),
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
            Interrupt::Memory(..) => write!(f, "Memory(..)"),
            Interrupt::Fs(..) => write!(f, "Fs(..)"),
        }
    }
}

/// An accumulated compile reason stored in the project state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompileReasons {
    /// The snapshot is taken by the memory editing events.
    pub by_memory_events: bool,
    /// The snapshot is taken by the file system events.
    pub by_fs_events: bool,
    /// The snapshot is taken by the entry change.
    pub by_entry_update: bool,
}

impl From<CompileReasons> for ExportSignal {
    fn from(value: CompileReasons) -> Self {
        Self {
            by_mem_events: value.by_memory_events,
            by_fs_events: value.by_fs_events,
            by_entry_update: value.by_entry_update,
        }
    }
}

impl CompileReasons {
    /// Merge two reasons.
    pub fn see(&mut self, reason: CompileReasons) {
        self.by_memory_events |= reason.by_memory_events;
        self.by_fs_events |= reason.by_fs_events;
        self.by_entry_update |= reason.by_entry_update;
    }

    /// Whether there is any reason to compile.
    pub fn any(&self) -> bool {
        self.by_memory_events || self.by_fs_events || self.by_entry_update
    }

    /// Exclude some reasons.
    pub fn exclude(&self, excluded: Self) -> Self {
        Self {
            by_memory_events: self.by_memory_events && !excluded.by_memory_events,
            by_fs_events: self.by_fs_events && !excluded.by_fs_events,
            by_entry_update: self.by_entry_update && !excluded.by_entry_update,
        }
    }
}

fn no_reason() -> CompileReasons {
    CompileReasons::default()
}

fn reason_by_mem() -> CompileReasons {
    CompileReasons {
        by_memory_events: true,
        ..CompileReasons::default()
    }
}

fn reason_by_fs() -> CompileReasons {
    CompileReasons {
        by_fs_events: true,
        ..CompileReasons::default()
    }
}

fn reason_by_entry_change() -> CompileReasons {
    CompileReasons {
        by_entry_update: true,
        ..CompileReasons::default()
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
    /// Whether to enable file system watching.
    pub enable_watch: bool,
    /// Specifies the current export target.
    pub export_target: ExportTarget,
}

impl<F: CompilerFeat + Send + Sync + 'static, Ext: 'static> Default for CompileServerOpts<F, Ext> {
    fn default() -> Self {
        Self {
            handler: Arc::new(std::marker::PhantomData),
            enable_watch: false,
            export_target: ExportTarget::Paged,
        }
    }
}

/// The synchronous compiler that runs on one project or multiple projects.
pub struct ProjectCompiler<F: CompilerFeat, Ext> {
    /// The compilation handle.
    pub handler: Arc<dyn CompileHandler<F, Ext>>,
    /// Specifies the current export target.
    export_target: ExportTarget,
    /// Channel for sending interrupts to the compiler actor.
    dep_tx: mpsc::UnboundedSender<NotifyMessage>,
    /// Whether to enable file system watching.
    pub enable_watch: bool,

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
            enable_watch,
            export_target,
        }: CompileServerOpts<F, Ext>,
    ) -> Self {
        let primary = Self::create_project(
            ProjectInsId("primary".into()),
            verse,
            export_target,
            handler.clone(),
        );
        Self {
            handler,
            dep_tx,
            enable_watch,
            export_target,

            logical_tick: 1,
            dirty_shadow_logical_tick: 0,

            estimated_shadow_files: Default::default(),

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
        ProjectInsState::run_compile(self.handler.clone(), snap, self.export_target)()
    }

    /// Gets the iterator of all projects.
    pub fn projects(&mut self) -> impl Iterator<Item = &mut ProjectInsState<F, Ext>> {
        std::iter::once(&mut self.primary).chain(self.dedicates.iter_mut())
    }

    fn create_project(
        id: ProjectInsId,
        verse: CompilerUniverse<F>,
        export_target: ExportTarget,
        handler: Arc<dyn CompileHandler<F, Ext>>,
    ) -> ProjectInsState<F, Ext> {
        ProjectInsState {
            id,
            ext: Default::default(),
            verse,
            reason: no_reason(),
            snapshot: None,
            handler,
            export_target,
            compilation: OnceLock::default(),
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
        );

        let mut proj =
            Self::create_project(id.clone(), verse, self.export_target, self.handler.clone());
        proj.reason.see(reason_by_entry_change());

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

                proj.reason.see(reason_by_entry_change());
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

                    // Reset the watch state and document state.
                    proj.latest_success_doc = None;
                }

                proj.reason.see(reason_by_entry_change());
            }

            Interrupt::Font(fonts) => {
                self.projects().for_each(|proj| {
                    let font_changed = proj.verse.increment_revision(|verse| {
                        verse.set_fonts(fonts.clone());
                        verse.font_changed()
                    });
                    if font_changed {
                        // todo: reason_by_font_change
                        proj.reason.see(reason_by_entry_change());
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
                            proj.reason.see(reason_by_mem());
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
            Interrupt::Fs(event) => {
                log::debug!("ProjectCompiler: fs event incoming {event:?}");

                // Apply file system changes.
                let dirty_tick = &mut self.dirty_shadow_logical_tick;
                let (changes, event) = event.split();
                let changes = std::iter::repeat_n(changes, 1 + self.dedicates.len());
                let proj = std::iter::once(&mut self.primary).chain(self.dedicates.iter_mut());

                for (proj, changes) in proj.zip(changes) {
                    let vfs_changed = proj.verse.increment_revision(|verse| {
                        {
                            let mut vfs = verse.vfs();

                            // Handle delayed upstream update event before applying file system
                            // changes
                            if Self::apply_delayed_memory_changes(&mut vfs, dirty_tick, &event)
                                .is_none()
                            {
                                log::warn!("ProjectCompiler: unknown upstream update event");

                                // Actual a delayed memory event.
                                proj.reason.see(reason_by_mem());
                            }
                            vfs.notify_fs_changes(changes);
                        }
                        verse.vfs_changed()
                    });

                    if vfs_changed {
                        proj.reason.see(reason_by_fs());
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
    /// The reason to compile.
    pub reason: CompileReasons,
    /// The latest compute graph (snapshot).
    snapshot: Option<Arc<WorldComputeGraph<F>>>,
    /// The latest compilation.
    pub compilation: OnceLock<CompiledArtifact<F>>,
    /// The compilation handle.
    pub handler: Arc<dyn CompileHandler<F, Ext>>,
    /// The file dependencies.
    deps: EcoVec<ImmutPath>,

    /// The latest successly compiled document.
    latest_success_doc: Option<TypstDocument>,

    committed_revision: usize,
}

impl<F: CompilerFeat, Ext: 'static> ProjectInsState<F, Ext> {
    /// Creates a snapshot of the project.
    pub fn snapshot(&mut self) -> Arc<WorldComputeGraph<F>> {
        match self.snapshot.as_ref() {
            Some(snap) if snap.world().revision() == self.verse.revision => snap.clone(),
            _ => {
                let snap = self.make_snapshot();
                self.snapshot = Some(snap.clone());
                snap
            }
        }
    }

    fn make_snapshot(&self) -> Arc<WorldComputeGraph<F>> {
        let world = self.verse.snapshot();
        let snap = CompileSnapshot {
            id: self.id.clone(),
            world,
            signal: ExportSignal {
                by_entry_update: self.reason.by_entry_update,
                by_mem_events: self.reason.by_memory_events,
                by_fs_events: self.reason.by_fs_events,
            },
            success_doc: self.latest_success_doc.clone(),
        };
        WorldComputeGraph::new(snap)
    }

    /// Compile the document once if there is any reason and the entry is
    /// active. (this is used for experimenting typst.node compilations)
    #[must_use]
    pub fn may_compile2(
        &mut self,
        compute: impl FnOnce(&Arc<WorldComputeGraph<F>>),
    ) -> Option<impl FnOnce() -> Arc<WorldComputeGraph<F>>> {
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

    /// Compile the document once if there is any reason and the entry is
    /// active.
    #[must_use]
    pub fn may_compile(
        &mut self,
        handler: &Arc<dyn CompileHandler<F, Ext>>,
    ) -> Option<impl FnOnce() -> CompiledArtifact<F>> {
        if !self.reason.any() || self.verse.entry_state().is_inactive() {
            return None;
        }

        let snap = self.snapshot();
        self.reason = Default::default();

        Some(Self::run_compile(handler.clone(), snap, self.export_target))
    }

    /// Compile the document once.
    fn run_compile(
        h: Arc<dyn CompileHandler<F, Ext>>,
        graph: Arc<WorldComputeGraph<F>>,
        export_target: ExportTarget,
    ) -> impl FnOnce() -> CompiledArtifact<F> {
        let start = tinymist_std::time::now();

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
            let compiled =
                CompiledArtifact::from_graph(graph, matches!(export_target, ExportTarget::Html));

            let res = CompileStatusResult {
                diag: (compiled.warning_cnt() + compiled.error_cnt()) as u32,
                elapsed: start.elapsed().unwrap_or_default(),
            };
            let rep = CompileReport {
                id: compiled.id().clone(),
                compiling_id: Some(id),
                page_count: compiled.doc.as_ref().map_or(0, |doc| doc.num_of_pages()),
                status: match &compiled.doc {
                    Some(..) => CompileStatusEnum::CompileSuccess(res),
                    None => CompileStatusEnum::CompileError(res),
                },
            };

            // todo: we need to check revision for really concurrent compilation
            log_compile_report(&rep);

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

        // Update state.
        let doc = artifact.doc.clone();
        self.committed_revision = compiled_revision;
        if doc.is_some() {
            self.latest_success_doc = doc;
        }

        // Notify the new file dependencies.
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
        rayon::spawn(move || {
            let evict_start = std::time::Instant::now();
            if is_primary {
                comemo::evict(10);

                // Since all the projects share the same cache, we need to evict the cache
                // on the primary instance for all the projects.
                world.evict_source_cache(30);
            }
            world.evict_vfs(60);
            let elapsed = evict_start.elapsed();
            log::info!("ProjectCompiler: evict cache in {elapsed:?}");
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
