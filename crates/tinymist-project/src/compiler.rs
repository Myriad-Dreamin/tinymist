//! Project Model for tinymist
//!
//! The [`ProjectCompiler`] implementation borrowed from typst.ts.
//!
//! Please check `tinymist::actor::typ_client` for architecture details.

#![allow(missing_docs)]

use std::{
    collections::HashSet,
    path::Path,
    sync::{Arc, OnceLock},
};

use tokio::sync::mpsc;

use ecow::{EcoString, EcoVec};
use tinymist_world::reflexo_typst::{
    features::{CompileFeature, FeatureSet, WITH_COMPILING_STATUS_FEATURE},
    CompileEnv, CompileReport, Compiler, CompilerFeat, TypstDocument, WorldDeps,
};
use tinymist_world::vfs::FsProvider;
use tinymist_world::{
    vfs::notify::{FilesystemEvent, MemoryEvent, NotifyMessage, UpstreamUpdateEvent},
    EntryReader,
};
use typst::diag::{SourceDiagnostic, SourceResult};

use tinymist_world::{CompilerUniverse, CompilerWorld, LspCompilerFeat, Revising, TaskInputs};

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
    fn notify_compile(&self, res: &CompiledArtifact<F>, rep: CompileReport);
    fn status(&self, revision: usize, rep: CompileReport);
}

/// No need so no compilation.
impl<F: CompilerFeat + Send + Sync + 'static, Ext: 'static> CompileHandler<F, Ext>
    for std::marker::PhantomData<fn(F, Ext)>
{
    fn on_any_compile_reason(&self, _state: &mut ProjectCompiler<F, Ext>) {
        log::info!("ProjectHandle: no need to compile");
    }
    fn notify_compile(&self, _res: &CompiledArtifact<F>, _rep: CompileReport) {}
    fn status(&self, _revision: usize, _rep: CompileReport) {}
}

pub enum Interrupt<F: CompilerFeat> {
    /// Compile anyway.
    Compile(ProjectInsId),
    /// Compiled from computing thread.
    Compiled(CompiledArtifact<F>),
    /// Change the watching entry.
    ChangeTask(TaskInputs),
    /// Font changes.
    Font(Arc<F::FontResolver>),
    /// Memory file changes.
    Memory(MemoryEvent),
    /// File system event.
    Fs(FilesystemEvent),
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
    /// The underlying universe.
    pub verse: CompilerUniverse<F>,
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
        let primary =
            Self::create_project(&verse, handler.clone(), dep_tx.clone(), feature_set.clone());
        Self {
            verse,
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
        verse: &CompilerUniverse<F>,
        handler: Arc<dyn CompileHandler<F, Ext>>,
        dep_tx: mpsc::UnboundedSender<NotifyMessage>,
        feature_set: FeatureSet,
    ) -> ProjectState<F, Ext> {
        let entry = verse.entry_state();
        ProjectState {
            id: ProjectInsId("primary".into()),
            ext: Default::default(),
            world: verse.snapshot(),
            reason: no_reason(),
            snapshot: OnceLock::new(),
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
            suspended: entry.is_inactive(),
            compiling: false,
            suspended_reason: no_reason(),
            committed_revision: 0,
        }
    }

    pub fn process(&mut self, intr: Interrupt<F>) {
        let previous_revision = self.verse.revision.get_mut().get();

        let reason = self.process_inner(intr);

        let revision = self.verse.revision.get_mut().get();
        if revision != previous_revision {
            let snap = self.verse.snapshot();
            for dedicate in &mut self.dedicates {
                dedicate.reset(snap.clone());
            }
            self.primary.reset(snap);
        }

        self.primary.reason.see(reason);
        for dedicate in &mut self.dedicates {
            dedicate.reason.see(reason);
        }

        // Customized Project Compilation Handler
        self.handler.clone().on_any_compile_reason(self);
    }

    pub fn snapshot(&mut self) -> CompileSnapshot<F> {
        self.primary.watch_snapshot()
    }

    /// Launches the compiler actor.
    pub fn start(&mut self) -> bool {
        log::debug!("ProjectCompiler: initialized");

        // Trigger the first compilation (if active)
        let compile = self
            .primary
            .run_compile_shared(reason_by_entry_change(), !self.enable_watch);
        if let Some(compile) = compile {
            compile();
        }

        true
    }

    /// Compile the document once.
    pub fn compile_once(&mut self) -> CompiledArtifact<F> {
        self.primary
            .run_compile_shared(reason_by_entry_change(), true)
            .expect("is_once is set")()
    }

    /// Compile the document once.
    pub fn run_compile_static(
        h: Arc<dyn CompileHandler<F, Ext>>,
        snap: CompileSnapshot<F>,
    ) -> impl FnOnce() -> CompiledArtifact<F> {
        let start = reflexo::time::now();

        // todo unwrap main id
        let id = snap.world.main_id().unwrap();
        let revision = snap.world.revision().get();

        h.status(revision, CompileReport::Stage(id, "compiling", start));

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

    /// Apply delayed memory changes to underlying compiler.
    fn apply_delayed_memory_changes(
        verse: &mut Revising<CompilerUniverse<F>>,
        dirty_shadow_logical_tick: &mut usize,
        event: &mut FilesystemEvent,
    ) -> Option<()> {
        // Handle delayed upstream update event before applying file system changes
        if let FilesystemEvent::UpstreamUpdate { upstream_event, .. } = event {
            let event = upstream_event.take()?.opaque;
            let TaggedMemoryEvent {
                logical_tick,
                event,
            } = *event.downcast().ok()?;

            // Recovery from dirty shadow state.
            if logical_tick == *dirty_shadow_logical_tick {
                *dirty_shadow_logical_tick = 0;
            }

            Self::apply_memory_changes(verse, event);
        }

        Some(())
    }

    /// Apply memory changes to underlying compiler.
    fn apply_memory_changes(verse: &mut Revising<CompilerUniverse<F>>, event: MemoryEvent) {
        if matches!(event, MemoryEvent::Sync(..)) {
            verse.reset_shadow();
        }
        match event {
            MemoryEvent::Update(event) | MemoryEvent::Sync(event) => {
                for removes in event.removes {
                    let _ = verse.unmap_shadow(&removes);
                }
                for (p, t) in event.inserts {
                    let insert_file = match t.content().cloned() {
                        Ok(content) => content,
                        Err(err) => {
                            log::error!("ProjectCompiler: read memory file at {p:?}: {err}");
                            continue;
                        }
                    };

                    let _ = verse.map_shadow(&p, insert_file);
                }
            }
        }
    }

    fn process_inner(&mut self, intr: Interrupt<F>) -> CompileReasons {
        match intr {
            Interrupt::Compile(id) => {
                let _ = id;
                // Increment the revision anyway.
                self.verse.increment_revision(|_| {});

                reason_by_entry_change()
            }
            Interrupt::ChangeTask(change) => {
                self.verse.increment_revision(|verse| {
                    if let Some(inputs) = change.inputs {
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
                    self.primary.suspended = entry.is_inactive();
                    if self.primary.suspended {
                        log::info!("ProjectCompiler: removing diag");
                        self.handler
                            .status(self.verse.revision.get_mut().get(), CompileReport::Suspend);
                    }

                    // Reset the watch state and document state.
                    self.primary.latest_doc = None;
                    self.primary.latest_success_doc = None;
                    self.primary.suspended_reason = no_reason();
                }

                reason_by_entry_change()
            }
            Interrupt::Compiled(artifact) => {
                self.primary.process_compile(artifact);
                self.primary.process_lagged_compile()
            }

            Interrupt::Font(font) => {
                self.verse.increment_revision(|verse| {
                    verse.inner.font_resolver = font;
                });

                reason_by_entry_change()
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
                    self.verse
                        .increment_revision(|verse| Self::apply_memory_changes(verse, event));
                    return reason_by_mem();
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

                no_reason()
            }
            Interrupt::Fs(mut event) => {
                log::debug!("ProjectCompiler: fs event incoming {event:?}");

                let mut reason = reason_by_fs();

                // Apply file system changes.
                let dirty_tick = &mut self.dirty_shadow_logical_tick;
                self.verse.increment_revision(|verse| {
                    // Handle delayed upstream update event before applying file system changes
                    if Self::apply_delayed_memory_changes(verse, dirty_tick, &mut event).is_none() {
                        log::warn!("ProjectCompiler: unknown upstream update event");

                        // Actual a delayed memory event.
                        reason = reason_by_mem();
                    }
                    verse.notify_fs_event(event)
                });

                reason
            }
        }
    }
}

pub struct ProjectState<F: CompilerFeat, Ext> {
    pub id: ProjectInsId,
    /// The extension
    pub ext: Ext,
    /// The forked world.
    pub world: CompilerWorld<F>,
    /// The reason to compile.
    pub reason: CompileReasons,
    /// The latest snapshot.
    pub snapshot: OnceLock<CompileSnapshot<F>>,
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

    suspended: bool,
    compiling: bool,
    suspended_reason: CompileReasons,
    committed_revision: usize,
}

impl<F: CompilerFeat + 'static, Ext: 'static> ProjectState<F, Ext> {
    pub fn make_env(&self, feature_set: Arc<FeatureSet>) -> CompileEnv {
        CompileEnv::default().configure_shared(feature_set)
    }

    pub fn snapshot(&self, is_once: bool) -> CompileSnapshot<F> {
        let world = self.world.clone();
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

    pub fn watch_snapshot(&mut self) -> CompileSnapshot<F> {
        // if self
        //     .watch_snap
        //     .get()
        //     .is_some_and(|e| e.world.revision() < *self.verse.revision.read())
        // {
        //     self.watch_snap = OnceLock::new();
        // }

        self.snapshot.get_or_init(|| self.snapshot(false)).clone()
    }

    /// Compile the document once.
    fn run_compile_shared(
        &mut self,
        reason: CompileReasons,
        is_once: bool,
    ) -> Option<impl FnOnce() -> CompiledArtifact<F>> {
        self.suspended_reason.see(reason);
        let reason = std::mem::take(&mut self.suspended_reason);
        let start = reflexo::time::now();

        let compiling = self.snapshot(is_once);
        self.reason = no_reason();
        self.snapshot = OnceLock::new();
        self.snapshot.get_or_init(|| compiling.clone());

        if self.suspended {
            self.suspended_reason.see(reason);
            return None;
        }

        if self.compiling {
            self.suspended_reason.see(reason);
            return None;
        }

        self.compiling = true;

        let h = self.handler.clone();

        // todo unwrap main id
        let id = compiling.world.main_id().unwrap();
        let revision = compiling.world.revision().get();

        h.status(revision, CompileReport::Stage(id, "compiling", start));

        Some(move || {
            let compiled = compiling.compile();

            let elapsed = start.elapsed().unwrap_or_default();
            let rep = match &compiled.doc {
                Ok(..) => CompileReport::CompileSuccess(id, compiled.warnings.clone(), elapsed),
                Err(err) => CompileReport::CompileError(id, err.clone(), elapsed),
            };

            // todo: we need to check revision for really concurrent compilation
            log_compile_report(&compiled.env, &rep);
            h.notify_compile(&compiled, rep);

            compiled
        })
    }

    fn process_compile(&mut self, artifact: CompiledArtifact<F>) {
        self.compiling = false;

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

        // Trigger an evict task.
        rayon::spawn(|| {
            let evict_start = std::time::Instant::now();
            comemo::evict(30);
            let elapsed = evict_start.elapsed();
            log::info!("CacheEvictTask: evict cache in {elapsed:?}");
        });
    }

    /// Process reason after each compilation.
    fn process_lagged_compile(&mut self) -> CompileReasons {
        // The reason which is kept but not used.
        std::mem::take(&mut self.suspended_reason)
    }

    fn reset(&mut self, world: CompilerWorld<F>) {
        self.world = world;
        self.world.set_is_compiling(false);

        self.snapshot = OnceLock::new();
        self.compilation = OnceLock::default();
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
