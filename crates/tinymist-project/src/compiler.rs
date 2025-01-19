//! Project Model for tinymist
//!
//! The [`ProjectCompiler`] implementation borrowed from typst.ts.
//!
//! Please check `tinymist::actor::typ_client` for architecture details.

#![allow(missing_docs)]

use core::fmt;
use std::{
    collections::HashSet,
    path::Path,
    sync::{Arc, OnceLock},
};

use ecow::{EcoString, EcoVec};
use reflexo_typst::{
    features::{CompileFeature, FeatureSet, WITH_COMPILING_STATUS_FEATURE},
    CompileEnv, CompileReport, Compiler, TypstDocument,
};
use tinymist_std::error::prelude::ZResult;
use tokio::sync::mpsc;
use typst::diag::{SourceDiagnostic, SourceResult};

use crate::LspCompilerFeat;
use tinymist_world::{
    vfs::{
        notify::{FilesystemEvent, MemoryEvent, NotifyMessage, UpstreamUpdateEvent},
        FsProvider, RevisingVfs,
    },
    CompilerFeat, CompilerUniverse, CompilerWorld, EntryReader, EntryState, TaskInputs, WorldDeps,
};

/// LSP interrupt.
pub type LspInterrupt = Interrupt<LspCompilerFeat>;

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct ProjectInsId(EcoString);

/// A signal that possibly triggers an export.
///
/// Whether to export depends on the current state of the document and the user
/// settings.
#[derive(Debug, Clone, Copy)]
pub struct ExportSignal {
    /// Whether the revision is annotated by memory events.
    pub by_mem_events: bool,
    /// Whether the revision is annotated by file system events.
    pub by_fs_events: bool,
    /// Whether the revision is annotated by entry update.
    pub by_entry_update: bool,
}

pub struct CompileSnapshot<F: CompilerFeat> {
    /// The project id.
    pub id: ProjectInsId,
    /// The export signal for the document.
    pub signal: ExportSignal,
    /// Using env
    pub env: CompileEnv,
    /// Using world
    pub world: CompilerWorld<F>,
    /// The last successfully compiled document.
    pub success_doc: Option<Arc<TypstDocument>>,
}

impl<F: CompilerFeat + 'static> CompileSnapshot<F> {
    pub fn task(mut self, inputs: TaskInputs) -> Self {
        'check_changed: {
            if let Some(entry) = &inputs.entry {
                if *entry != self.world.entry_state() {
                    break 'check_changed;
                }
            }
            if let Some(inputs) = &inputs.inputs {
                if inputs.clone() != self.world.inputs() {
                    break 'check_changed;
                }
            }

            return self;
        };

        self.world = self.world.task(inputs);

        self
    }

    pub fn compile(self) -> CompiledArtifact<F> {
        let mut snap = self;
        snap.world.set_is_compiling(true);
        let warned = std::marker::PhantomData.compile(&snap.world, &mut snap.env);
        snap.world.set_is_compiling(false);
        let (doc, warnings) = match warned {
            Ok(doc) => (Ok(doc.output), doc.warnings),
            Err(err) => (Err(err), EcoVec::default()),
        };
        CompiledArtifact {
            snap,
            doc,
            warnings,
        }
    }
}

impl<F: CompilerFeat> Clone for CompileSnapshot<F> {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            signal: self.signal,
            env: self.env.clone(),
            world: self.world.clone(),
            success_doc: self.success_doc.clone(),
        }
    }
}

pub struct CompiledArtifact<F: CompilerFeat> {
    /// The used snapshot.
    pub snap: CompileSnapshot<F>,
    /// The diagnostics of the document.
    pub warnings: EcoVec<SourceDiagnostic>,
    /// The compiled document.
    pub doc: SourceResult<Arc<TypstDocument>>,
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
        }
    }
}

impl<F: CompilerFeat> CompiledArtifact<F> {
    pub fn success_doc(&self) -> Option<Arc<TypstDocument>> {
        self.doc
            .as_ref()
            .ok()
            .cloned()
            .or_else(|| self.snap.success_doc.clone())
    }
}

pub trait CompileHandler<F: CompilerFeat, Ext>: Send + Sync + 'static {
    fn on_any_compile_reason(&self, state: &mut ProjectCompiler<F, Ext>);
    // todo: notify project specific compile
    fn notify_compile(&self, res: &CompiledArtifact<F>, rep: CompileReport);
    fn status(&self, revision: usize, id: &ProjectInsId, rep: CompileReport);
}

/// No need so no compilation.
impl<F: CompilerFeat + Send + Sync + 'static, Ext: 'static> CompileHandler<F, Ext>
    for std::marker::PhantomData<fn(F, Ext)>
{
    fn on_any_compile_reason(&self, _state: &mut ProjectCompiler<F, Ext>) {
        log::info!("ProjectHandle: no need to compile");
    }
    fn notify_compile(&self, _res: &CompiledArtifact<F>, _rep: CompileReport) {}
    fn status(&self, _revision: usize, _id: &ProjectInsId, _rep: CompileReport) {}
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompileReasons {
    /// The snapshot is taken by the memory editing events.
    pub by_memory_events: bool,
    /// The snapshot is taken by the file system events.
    pub by_fs_events: bool,
    /// The snapshot is taken by the entry change.
    pub by_entry_update: bool,
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

pub struct CompileServerOpts<F: CompilerFeat, Ext> {
    pub handler: Arc<dyn CompileHandler<F, Ext>>,
    pub feature_set: FeatureSet,
    pub enable_watch: bool,
}

impl<F: CompilerFeat + Send + Sync + 'static, Ext: 'static> Default for CompileServerOpts<F, Ext> {
    fn default() -> Self {
        Self {
            handler: Arc::new(std::marker::PhantomData),
            feature_set: Default::default(),
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
    pub primary: ProjectState<F, Ext>,
    /// The states for dedicate tasks
    pub dedicates: Vec<ProjectState<F, Ext>>,
}

impl<F: CompilerFeat + Send + Sync + 'static, Ext: Default + 'static> ProjectCompiler<F, Ext> {
    /// Create a compiler with options
    pub fn new(
        verse: CompilerUniverse<F>,
        dep_tx: mpsc::UnboundedSender<NotifyMessage>,
        CompileServerOpts {
            handler,
            feature_set,
            enable_watch,
        }: CompileServerOpts<F, Ext>,
    ) -> Self {
        let primary = Self::create_project(
            ProjectInsId("primary".into()),
            verse,
            handler.clone(),
            dep_tx.clone(),
            feature_set.clone(),
        );
        Self {
            handler,
            dep_tx,
            enable_watch,

            logical_tick: 1,
            dirty_shadow_logical_tick: 0,

            estimated_shadow_files: Default::default(),

            primary,
            dedicates: vec![],
        }
    }

    fn create_project(
        id: ProjectInsId,
        verse: CompilerUniverse<F>,
        handler: Arc<dyn CompileHandler<F, Ext>>,
        dep_tx: mpsc::UnboundedSender<NotifyMessage>,
        feature_set: FeatureSet,
    ) -> ProjectState<F, Ext> {
        ProjectState {
            id,
            ext: Default::default(),
            verse,
            reason: no_reason(),
            snapshot: None,
            handler,
            dep_tx,
            compilation: OnceLock::default(),
            latest_doc: None,
            latest_success_doc: None,
            once_feature_set: Arc::new(feature_set.clone()),
            watch_feature_set: Arc::new(
                feature_set
                    .clone()
                    .configure(&WITH_COMPILING_STATUS_FEATURE, true),
            ),
            committed_revision: 0,
        }
    }

    pub fn process(&mut self, intr: Interrupt<F>) {
        // todo: evcit cache
        self.process_inner(intr);
        // Customized Project Compilation Handler
        self.handler.clone().on_any_compile_reason(self);
    }

    pub fn snapshot(&mut self) -> CompileSnapshot<F> {
        self.primary.snapshot()
    }

    /// Compile the document once.
    pub fn compile_once(&mut self) -> CompiledArtifact<F> {
        let snap = self.primary.make_snapshot(true);
        ProjectState::run_compile(self.handler.clone(), snap)()
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

    fn find_project<'a>(
        primary: &'a mut ProjectState<F, Ext>,
        dedicates: &'a mut [ProjectState<F, Ext>],
        id: &ProjectInsId,
    ) -> &'a mut ProjectState<F, Ext> {
        if id == &primary.id {
            return primary;
        }

        dedicates.iter_mut().find(|e| e.id == *id).unwrap()
    }

    pub fn projects(&mut self) -> impl Iterator<Item = &mut ProjectState<F, Ext>> {
        std::iter::once(&mut self.primary).chain(self.dedicates.iter_mut())
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

                proj.process_compile(artifact);
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
                        let vfs_changed = proj.verse.increment_revision(|verse| {
                            Self::apply_memory_changes(&mut verse.vfs(), event.clone());
                            verse.vfs_changed()
                        });
                        if vfs_changed {
                            proj.reason.see(reason_by_mem());
                        }
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

    pub fn restart_dedicate(&mut self, group: &str, entry: EntryState) -> ZResult<ProjectInsId> {
        let id = ProjectInsId(group.into());

        let verse = CompilerUniverse::<F>::new_raw(
            entry,
            Some(self.primary.verse.inputs().clone()),
            self.primary.verse.vfs().fork(),
            self.primary.verse.registry.clone(),
            self.primary.verse.font_resolver.clone(),
        );

        let proj = Self::create_project(
            id.clone(),
            verse,
            self.handler.clone(),
            self.dep_tx.clone(),
            self.primary.once_feature_set.as_ref().to_owned(),
        );

        self.remove_dedicates(&id);
        self.dedicates.push(proj);

        Ok(id)
    }

    fn remove_dedicates(&mut self, id: &ProjectInsId) {
        let proj = self.dedicates.iter().position(|e| e.id == *id);
        if let Some(idx) = proj {
            let _proj = self.dedicates.remove(idx);
            // todo: kill compilations
        } else {
            log::warn!("ProjectCompiler: settle project not found {id:?}");
        }
    }
}

pub struct ProjectState<F: CompilerFeat, Ext> {
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
    /// Channel for sending interrupts to the compiler actor.
    dep_tx: mpsc::UnboundedSender<NotifyMessage>,

    /// The latest compiled document.
    pub(crate) latest_doc: Option<Arc<TypstDocument>>,
    /// The latest successly compiled document.
    latest_success_doc: Option<Arc<TypstDocument>>,
    /// feature set for compile_once mode.
    once_feature_set: Arc<FeatureSet>,
    /// Shared feature set for watch mode.
    watch_feature_set: Arc<FeatureSet>,

    committed_revision: usize,
}

impl<F: CompilerFeat, Ext: 'static> ProjectState<F, Ext> {
    pub fn make_env(&self, feature_set: Arc<FeatureSet>) -> CompileEnv {
        CompileEnv::default().configure_shared(feature_set)
    }

    pub fn snapshot(&mut self) -> CompileSnapshot<F> {
        match self.snapshot.as_ref() {
            Some(snap) if snap.world.revision() == self.verse.revision => snap.clone(),
            _ => {
                let snap = self.make_snapshot(false);
                self.snapshot = Some(snap.clone());
                snap
            }
        }
    }

    fn make_snapshot(&self, is_once: bool) -> CompileSnapshot<F> {
        let world = self.verse.snapshot();
        let env = self.make_env(if is_once {
            self.once_feature_set.clone()
        } else {
            self.watch_feature_set.clone()
        });
        CompileSnapshot {
            id: self.id.clone(),
            world,
            env: env.clone(),
            signal: ExportSignal {
                by_entry_update: self.reason.by_entry_update,
                by_mem_events: self.reason.by_memory_events,
                by_fs_events: self.reason.by_fs_events,
            },
            success_doc: self.latest_success_doc.clone(),
        }
    }

    fn process_compile(&mut self, artifact: CompiledArtifact<F>) {
        let world = &artifact.snap.world;
        let compiled_revision = world.revision().get();
        if self.committed_revision >= compiled_revision {
            return;
        }

        // Update state.
        let doc = artifact.doc.ok();
        self.committed_revision = compiled_revision;
        self.latest_doc.clone_from(&doc);
        if doc.is_some() {
            self.latest_success_doc.clone_from(&self.latest_doc);
        }

        // Notify the new file dependencies.
        let mut deps = vec![];
        world.iter_dependencies(&mut |dep| {
            if let Ok(x) = world.file_path(dep).and_then(|e| e.to_err()) {
                deps.push(x.into())
            }
        });
        let event = NotifyMessage::SyncDependency(deps);
        let err = self.dep_tx.send(event);
        log_send_error("dep_tx", err);

        let mut world = artifact.snap.world;

        let rev = world.revision();
        let cache = world.take_cache();
        let is_primary = self.id == ProjectInsId("primary".into());

        // Trigger an evict task.
        rayon::spawn(move || {
            let evict_start = std::time::Instant::now();
            if is_primary {
                comemo::evict(10);
            }
            cache.evict(rev, 10);
            let elapsed = evict_start.elapsed();
            log::info!("CacheEvictTask: evict cache in {elapsed:?}");
        });
    }

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
            let compiled = snap.compile();

            let elapsed = start.elapsed().unwrap_or_default();
            let rep = match &compiled.doc {
                Ok(..) => CompileReport::CompileSuccess(id, compiled.warnings.clone(), elapsed),
                Err(err) => CompileReport::CompileError(id, err.clone(), elapsed),
            };

            // todo: we need to check revision for really concurrent compilation
            log_compile_report(&compiled.env, &rep);
            h.notify_compile(&compiled, rep);

            compiled
        }
    }
}

fn log_compile_report(env: &CompileEnv, rep: &CompileReport) {
    if WITH_COMPILING_STATUS_FEATURE.retrieve(&env.features) {
        log::info!("{}", rep.message());
    }
}

#[inline]
fn log_send_error<T>(chan: &'static str, res: Result<(), mpsc::error::SendError<T>>) -> bool {
    res.map_err(|err| log::warn!("ProjectCompiler: send to {chan} error: {err}"))
        .is_ok()
}
