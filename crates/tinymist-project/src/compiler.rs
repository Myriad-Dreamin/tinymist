//! Project compiler for tinymist.

use core::fmt;
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use ecow::{eco_vec, EcoVec};
use tinymist_std::error::prelude::Result;
use tinymist_std::{typst::TypstDocument, ImmutPath};
use tinymist_world::vfs::notify::{
    FilesystemEvent, MemoryEvent, NotifyDeps, NotifyMessage, UpstreamUpdateEvent,
};
use tinymist_world::vfs::{FileId, FsProvider, RevisingVfs, WorkspaceResolver};
use tinymist_world::{
    CompileSnapshot, CompilerFeat, CompilerUniverse, EntryReader, EntryState, ExportSignal,
    ProjectInsId, TaskInputs, WorldDeps,
};
use tokio::sync::mpsc;
use typst::diag::{SourceDiagnostic, SourceResult, Warned};

use crate::LspCompilerFeat;

/// LSP compile snapshot.
pub type LspCompileSnapshot = CompileSnapshot<LspCompilerFeat>;
/// LSP compiled artifact.
pub type LspCompiledArtifact = CompiledArtifact<LspCompilerFeat>;
/// LSP interrupt.
pub type LspInterrupt = Interrupt<LspCompilerFeat>;
/// A compiled artifact.
pub struct CompiledArtifact<F: CompilerFeat> {
    /// The used snapshot.
    pub snap: CompileSnapshot<F>,
    /// The diagnostics of the document.
    pub warnings: EcoVec<SourceDiagnostic>,
    /// The compiled document.
    pub doc: SourceResult<TypstDocument>,
    /// The depended files.
    pub deps: OnceLock<EcoVec<FileId>>,
}

impl fmt::Display for CompiledArtifact<LspCompilerFeat> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rev = self.world.revision();
        write!(f, "CompiledArtifact({:?}, rev={rev:?})", self.id)
    }
}

impl<F: CompilerFeat> std::ops::Deref for CompiledArtifact<F> {
    type Target = CompileSnapshot<F>;

    fn deref(&self) -> &Self::Target {
        &self.snap
    }
}

impl<F: CompilerFeat> Clone for CompiledArtifact<F> {
    fn clone(&self) -> Self {
        Self {
            snap: self.snap.clone(),
            doc: self.doc.clone(),
            warnings: self.warnings.clone(),
            deps: self.deps.clone(),
        }
    }
}

impl<F: CompilerFeat> CompiledArtifact<F> {
    /// Returns the last successfully compiled document.
    pub fn success_doc(&self) -> Option<TypstDocument> {
        self.doc
            .as_ref()
            .ok()
            .cloned()
            .or_else(|| self.snap.success_doc.clone())
    }

    /// Returns the depended files.
    pub fn depended_files(&self) -> &EcoVec<FileId> {
        self.deps.get_or_init(|| {
            let mut deps = EcoVec::default();
            self.world.iter_dependencies(&mut |f| {
                deps.push(f);
            });

            deps
        })
    }

    /// Runs the compiler and returns the compiled document.
    pub fn from_snapshot(mut snap: CompileSnapshot<F>) -> CompiledArtifact<F> {
        snap.world.set_is_compiling(true);
        let res = ::typst::compile::<tinymist_std::typst::TypstPagedDocument>(&snap.world);
        let warned = match res.output {
            Ok(doc) => Ok(Warned {
                output: Arc::new(doc),
                warnings: res.warnings,
            }),
            Err(diags) => match (res.warnings.is_empty(), diags.is_empty()) {
                (true, true) => Err(diags),
                (true, false) => Err(diags),
                (false, true) => Err(res.warnings),
                (false, false) => {
                    let mut warnings = res.warnings;
                    warnings.extend(diags);
                    Err(warnings)
                }
            },
        };
        snap.world.set_is_compiling(false);
        let (doc, warnings) = match warned {
            Ok(doc) => (Ok(TypstDocument::Paged(doc.output)), doc.warnings),
            Err(err) => (Err(err), EcoVec::default()),
        };
        CompiledArtifact {
            snap,
            doc,
            warnings,
            deps: OnceLock::default(),
        }
    }
}

// todo: remove me
#[allow(missing_docs)]
#[derive(Clone, Debug)]
pub enum CompileReport {
    Suspend,
    Stage(FileId, &'static str, tinymist_std::time::Time),
    CompileError(FileId, usize, tinymist_std::time::Duration),
    ExportError(FileId, usize, tinymist_std::time::Duration),
    CompileSuccess(
        FileId,
        // warnings, if not empty
        usize,
        tinymist_std::time::Duration,
    ),
}

#[allow(missing_docs)]
impl CompileReport {
    pub fn compiling_id(&self) -> Option<FileId> {
        Some(match self {
            Self::Suspend => return None,
            Self::Stage(id, ..)
            | Self::CompileError(id, ..)
            | Self::ExportError(id, ..)
            | Self::CompileSuccess(id, ..) => *id,
        })
    }

    pub fn duration(&self) -> Option<std::time::Duration> {
        match self {
            Self::Suspend | Self::Stage(..) => None,
            Self::CompileError(_, _, dur)
            | Self::ExportError(_, _, dur)
            | Self::CompileSuccess(_, _, dur) => Some(*dur),
        }
    }

    pub fn diagnostics_size(self) -> Option<usize> {
        match self {
            Self::Suspend | Self::Stage(..) => None,
            Self::CompileError(_, diagnostics, ..)
            | Self::ExportError(_, diagnostics, ..)
            | Self::CompileSuccess(_, diagnostics, ..) => Some(diagnostics),
        }
    }

    /// Get the status message.
    pub fn message(&self) -> CompileReportMsg<'_> {
        CompileReportMsg(self)
    }
}

#[allow(missing_docs)]
pub struct CompileReportMsg<'a>(&'a CompileReport);

impl fmt::Display for CompileReportMsg<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use CompileReport::*;

        let input = WorkspaceResolver::display(self.0.compiling_id());
        match self.0 {
            Suspend => write!(f, "suspended"),
            Stage(_, stage, ..) => write!(f, "{input:?}: {stage} ..."),
            CompileSuccess(_, warnings, duration) => {
                if *warnings == 0 {
                    write!(f, "{input:?}: compilation succeeded in {duration:?}")
                } else {
                    write!(
                        f,
                        "{input:?}: compilation succeeded with {warnings} warnings in {duration:?}",
                    )
                }
            }
            CompileError(_, _, duration) | ExportError(_, _, duration) => {
                write!(f, "{input:?}: compilation failed after {duration:?}")
            }
        }
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
    fn status(&self, revision: usize, id: &ProjectInsId, rep: CompileReport);
}

/// No need so no compilation.
impl<F: CompilerFeat + Send + Sync + 'static, Ext: 'static> CompileHandler<F, Ext>
    for std::marker::PhantomData<fn(F, Ext)>
{
    fn on_any_compile_reason(&self, _state: &mut ProjectCompiler<F, Ext>) {
        log::info!("ProjectHandle: no need to compile");
    }
    fn notify_compile(&self, _res: &CompiledArtifact<F>) {}
    fn status(&self, _revision: usize, _id: &ProjectInsId, _rep: CompileReport) {}
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

impl fmt::Debug for Interrupt<LspCompilerFeat> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Interrupt::Compile(id) => write!(f, "Compile({id:?})"),
            Interrupt::Settle(id) => write!(f, "Settle({id:?})"),
            Interrupt::Compiled(artifact) => write!(f, "Compiled({:?})", artifact.id),
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
}

impl<F: CompilerFeat + Send + Sync + 'static, Ext: 'static> Default for CompileServerOpts<F, Ext> {
    fn default() -> Self {
        Self {
            handler: Arc::new(std::marker::PhantomData),
            enable_watch: false,
        }
    }
}

/// The synchronous compiler that runs on one project or multiple projects.
pub struct ProjectCompiler<F: CompilerFeat, Ext> {
    /// The compilation handle.
    pub handler: Arc<dyn CompileHandler<F, Ext>>,
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
        }: CompileServerOpts<F, Ext>,
    ) -> Self {
        let primary = Self::create_project(ProjectInsId("primary".into()), verse, handler.clone());
        Self {
            handler,
            dep_tx,
            enable_watch,

            logical_tick: 1,
            dirty_shadow_logical_tick: 0,

            estimated_shadow_files: Default::default(),

            primary,
            deps: Default::default(),
            dedicates: vec![],
        }
    }

    /// Creates a snapshot of the primary project.
    pub fn snapshot(&mut self) -> CompileSnapshot<F> {
        self.primary.snapshot()
    }

    /// Compiles the document once.
    pub fn compile_once(&mut self) -> CompiledArtifact<F> {
        let snap = self.primary.make_snapshot();
        ProjectInsState::run_compile(self.handler.clone(), snap)()
    }

    /// Gets the iterator of all projects.
    pub fn projects(&mut self) -> impl Iterator<Item = &mut ProjectInsState<F, Ext>> {
        std::iter::once(&mut self.primary).chain(self.dedicates.iter_mut())
    }

    fn create_project(
        id: ProjectInsId,
        verse: CompilerUniverse<F>,
        handler: Arc<dyn CompileHandler<F, Ext>>,
    ) -> ProjectInsState<F, Ext> {
        ProjectInsState {
            id,
            ext: Default::default(),
            verse,
            reason: no_reason(),
            snapshot: None,
            handler,
            compilation: OnceLock::default(),
            latest_doc: None,
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

    /// Restart a dedicate project.
    pub fn restart_dedicate(&mut self, group: &str, entry: EntryState) -> Result<ProjectInsId> {
        let id = ProjectInsId(group.into());

        let verse = CompilerUniverse::<F>::new_raw(
            entry,
            Some(self.primary.verse.inputs().clone()),
            self.primary.verse.vfs().fork(),
            self.primary.verse.registry.clone(),
            self.primary.verse.font_resolver.clone(),
        );

        let proj = Self::create_project(id.clone(), verse, self.handler.clone());

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
                let proj = Self::find_project(&mut self.primary, &mut self.dedicates, &artifact.id);

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
                        self.handler.status(
                            proj.verse.revision.get(),
                            &proj.id,
                            CompileReport::Suspend,
                        );
                    }

                    // Reset the watch state and document state.
                    proj.latest_doc = None;
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
                    for (proj, event) in std::iter::once(&mut self.primary).zip(changes) {
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
    /// The reason to compile.
    pub reason: CompileReasons,
    /// The latest snapshot.
    snapshot: Option<CompileSnapshot<F>>,
    /// The latest compilation.
    pub compilation: OnceLock<CompiledArtifact<F>>,
    /// The compilation handle.
    pub handler: Arc<dyn CompileHandler<F, Ext>>,
    /// The file dependencies.
    deps: EcoVec<ImmutPath>,

    /// The latest compiled document.
    pub(crate) latest_doc: Option<TypstDocument>,
    /// The latest successly compiled document.
    latest_success_doc: Option<TypstDocument>,

    committed_revision: usize,
}

impl<F: CompilerFeat, Ext: 'static> ProjectInsState<F, Ext> {
    /// Creates a snapshot of the project.
    pub fn snapshot(&mut self) -> CompileSnapshot<F> {
        match self.snapshot.as_ref() {
            Some(snap) if snap.world.revision() == self.verse.revision => snap.clone(),
            _ => {
                let snap = self.make_snapshot();
                self.snapshot = Some(snap.clone());
                snap
            }
        }
    }

    fn make_snapshot(&self) -> CompileSnapshot<F> {
        let world = self.verse.snapshot();
        CompileSnapshot {
            id: self.id.clone(),
            world,
            signal: ExportSignal {
                by_entry_update: self.reason.by_entry_update,
                by_mem_events: self.reason.by_memory_events,
                by_fs_events: self.reason.by_fs_events,
            },
            success_doc: self.latest_success_doc.clone(),
        }
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

        Some(Self::run_compile(handler.clone(), snap))
    }

    /// Compile the document once.
    fn run_compile(
        h: Arc<dyn CompileHandler<F, Ext>>,
        snap: CompileSnapshot<F>,
    ) -> impl FnOnce() -> CompiledArtifact<F> {
        let start = tinymist_std::time::now();

        // todo unwrap main id
        let id = snap.world.main_id().unwrap();
        let revision = snap.world.revision().get();

        h.status(
            revision,
            &snap.id,
            CompileReport::Stage(id, "compiling", start),
        );

        move || {
            let compiled = CompiledArtifact::from_snapshot(snap);

            let elapsed = start.elapsed().unwrap_or_default();
            let rep = match &compiled.doc {
                Ok(..) => CompileReport::CompileSuccess(id, compiled.warnings.len(), elapsed),
                Err(err) => CompileReport::CompileError(id, err.len(), elapsed),
            };

            // todo: we need to check revision for really concurrent compilation
            log_compile_report(&rep);

            h.status(revision, &compiled.id, rep);
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
        let doc = artifact.doc.ok();
        self.committed_revision = compiled_revision;
        self.latest_doc.clone_from(&doc);
        if doc.is_some() {
            self.latest_success_doc.clone_from(&self.latest_doc);
        }

        // Notify the new file dependencies.
        let mut deps = eco_vec![];
        world.iter_dependencies(&mut |dep| {
            if let Ok(x) = world.file_path(dep).and_then(|e| e.to_err()) {
                deps.push(x.into())
            }
        });

        self.deps = deps.clone();

        let mut world = artifact.snap.world;

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
