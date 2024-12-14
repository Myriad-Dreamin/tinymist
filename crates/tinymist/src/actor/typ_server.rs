//! The [`CompileServerActor`] implementation borrowed from typst.ts.
//!
//! Please check `tinymist::actor::typ_client` for architecture details.

use parking_lot::Mutex;
use std::{
    collections::HashSet,
    ops::Deref,
    path::Path,
    sync::{Arc, OnceLock},
};
use tokio::sync::{mpsc, oneshot};

use reflexo_typst::{
    features::{FeatureSet, WITH_COMPILING_STATUS_FEATURE},
    typst::prelude::EcoVec,
    vfs::notify::{FilesystemEvent, MemoryEvent, NotifyMessage, UpstreamUpdateEvent},
    watch_deps,
    world::{CompilerFeat, CompilerUniverse, CompilerWorld},
    CompileEnv, CompileReport, Compiler, ConsoleDiagReporter, EntryReader, EntryState,
    GenericExporter, LazyHash, Revising, TaskInputs, TypstDict, TypstDocument, WorldDeps,
};
use typst::diag::{SourceDiagnostic, SourceResult};
use typst_shim::utils::Deferred;

use crate::task::CacheTask;

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
    /// The export signal for the document.
    pub flags: ExportSignal,
    /// Using env
    pub env: CompileEnv,
    /// Using world
    pub world: Arc<CompilerWorld<F>>,
    /// Compiling the document.
    compiled_doc: Arc<Mutex<Option<CompiledArtifact<F>>>>,
    doc_state: Arc<OnceLock<Deferred<CompiledArtifact<F>>>>,
    /// Compiling the document.
    /// The last successfully compiled document.
    pub success_doc: Option<Arc<TypstDocument>>,
}

impl<F: CompilerFeat + 'static> CompileSnapshot<F> {
    fn start(
        &self,
        before: impl FnOnce(&Self) + Send + Sync + 'static,
        after: impl FnOnce(&CompiledArtifact<F>) + Send + Sync + 'static,
    ) -> &Deferred<CompiledArtifact<F>> {
        self.doc_state.get_or_init(|| {
            let this = self.clone();
            Deferred::new(move || {
                let mut this = this;
                before(&this);
                let w = this.world.as_ref();
                let mut c = std::marker::PhantomData;
                let doc = c.compile(w, &mut this.env);

                // let (doc, env) = self.start(f).wait().clone();
                let (doc, warnings) = match doc {
                    Ok(doc) => (Ok(doc.output), doc.warnings),
                    Err(err) => (Err(err), EcoVec::default()),
                };
                let res = CompiledArtifact {
                    signal: this.flags,
                    world: this.world.clone(),
                    env: this.env,
                    doc,
                    warnings,
                    success_doc: this.success_doc.clone(),
                };

                log::info!("CompileSnapshot: compiled doc");
                this.compiled_doc.lock().clone_from(&Some(res.clone()));
                after(&res);

                res
            })
        })
    }

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

        self.world = Arc::new(self.world.task(inputs));
        self.doc_state = Arc::new(OnceLock::new());

        self
    }

    pub fn compile(&self) -> CompiledArtifact<F> {
        self.start(|_| {}, |_| {}).wait().clone()
    }

    pub fn compile_with(
        &self,
        before: impl FnOnce(&Self) + Send + Sync + 'static,
        after: impl FnOnce(&CompiledArtifact<F>) + Send + Sync + 'static,
    ) -> CompiledArtifact<F> {
        self.start(before, after).wait().clone()
    }
}

impl<F: CompilerFeat> Clone for CompileSnapshot<F> {
    fn clone(&self) -> Self {
        Self {
            flags: self.flags,
            env: self.env.clone(),
            world: self.world.clone(),
            doc_state: self.doc_state.clone(),
            compiled_doc: self.compiled_doc.clone(),
            success_doc: self.success_doc.clone(),
        }
    }
}

pub struct CompiledArtifact<F: CompilerFeat> {
    /// All the export signal for the document.
    pub signal: ExportSignal,
    /// Used world
    pub world: Arc<CompilerWorld<F>>,
    /// Used env
    pub env: CompileEnv,
    /// The diagnostics of the document.
    pub warnings: EcoVec<SourceDiagnostic>,
    /// The compiled document.
    pub doc: SourceResult<Arc<TypstDocument>>,
    /// The last successfully compiled document.
    pub success_doc: Option<Arc<TypstDocument>>,
}

impl<F: CompilerFeat> Clone for CompiledArtifact<F> {
    fn clone(&self) -> Self {
        Self {
            signal: self.signal,
            world: self.world.clone(),
            env: self.env.clone(),
            doc: self.doc.clone(),
            warnings: self.warnings.clone(),
            success_doc: self.success_doc.clone(),
        }
    }
}

impl<F: CompilerFeat> CompiledArtifact<F> {
    pub fn success_doc(&self) -> Option<Arc<TypstDocument>> {
        self.doc
            .as_ref()
            .ok()
            .cloned()
            .or_else(|| self.success_doc.clone())
    }
}

pub trait CompilationHandle<F: CompilerFeat>: Send + Sync + 'static {
    fn status(&self, revision: usize, rep: CompileReport);
    fn notify_compile(&self, res: &CompiledArtifact<F>, rep: CompileReport);
    fn restore_compile(&self, res: &CompiledArtifact<F>, rep: CompileReport);
}

impl<F: CompilerFeat + Send + Sync + 'static> CompilationHandle<F>
    for std::marker::PhantomData<fn(F)>
{
    fn status(&self, _revision: usize, _: CompileReport) {}
    fn notify_compile(&self, _: &CompiledArtifact<F>, _: CompileReport) {}
    fn restore_compile(&self, _: &CompiledArtifact<F>, _: CompileReport) {}
}

pub enum SucceededArtifact<F: CompilerFeat> {
    Compiled(CompiledArtifact<F>),
    Suspend(CompileSnapshot<F>),
}

impl<F: CompilerFeat> SucceededArtifact<F> {
    pub fn success_doc(&self) -> Option<Arc<TypstDocument>> {
        match self {
            SucceededArtifact::Compiled(artifact) => artifact.success_doc(),
            SucceededArtifact::Suspend(snapshot) => snapshot.success_doc.clone(),
        }
    }

    pub fn world(&self) -> &Arc<CompilerWorld<F>> {
        match self {
            SucceededArtifact::Compiled(artifact) => &artifact.world,
            SucceededArtifact::Suspend(snapshot) => &snapshot.world,
        }
    }
}

pub enum Interrupt<F: CompilerFeat> {
    /// Compile anyway.
    Compile,
    /// Compiled from computing thread.
    Compiled(CompiledArtifact<F>),
    /// Change the watching entry.
    ChangeTask(TaskInputs),
    /// Request compiler to respond a snapshot without needing to wait latest
    /// compilation.
    SnapshotRead(oneshot::Sender<CompileSnapshot<F>>),
    /// Request compiler to respond a snapshot with at least a compilation
    /// happens on or after current revision.
    CurrentRead(oneshot::Sender<SucceededArtifact<F>>),
    /// Memory file changes.
    Memory(MemoryEvent),
    /// File system event.
    Fs(FilesystemEvent),
    /// Request compiler to stop.
    Settle(oneshot::Sender<()>),
}

/// Responses from the compiler actor.
enum CompilerResponse {
    /// Response to the file watcher
    Notify(NotifyMessage),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct CompileReasons {
    /// The snapshot is taken by the memory editing events.
    by_memory_events: bool,
    /// The snapshot is taken by the file system events.
    by_fs_events: bool,
    /// The snapshot is taken by the entry change.
    by_entry_update: bool,
}

impl CompileReasons {
    /// Merge two reasons.
    fn merge(&mut self, reason: CompileReasons) {
        self.by_memory_events |= reason.by_memory_events;
        self.by_fs_events |= reason.by_fs_events;
        self.by_entry_update |= reason.by_entry_update;
    }

    /// Whether the behind reason is "file changed".
    fn file_changed(&self) -> bool {
        self.by_memory_events || self.by_fs_events
    }

    /// Whether we should compile for any reason.
    fn any(&self) -> bool {
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

pub struct CompileServerOpts<F: CompilerFeat> {
    pub compile_handle: Arc<dyn CompilationHandle<F>>,
    pub feature_set: FeatureSet,
    pub cache: CacheTask,
}

impl<F: CompilerFeat + Send + Sync + 'static> Default for CompileServerOpts<F> {
    fn default() -> Self {
        Self {
            compile_handle: Arc::new(std::marker::PhantomData),
            feature_set: Default::default(),
            cache: Default::default(),
        }
    }
}

#[derive(Clone)]
struct CompileState<F: CompilerFeat> {
    /// The revision of the state.
    compiled_at: usize,
    /// The snapshot for watching mode.
    watch_snap: OnceLock<CompileSnapshot<F>>,
    /// The compiled document.
    pub(crate) doc: Option<Arc<TypstDocument>>,
    /// The successly compiled document.
    success_doc: Option<Arc<TypstDocument>>,
    /// The reason why the compiler is suspended.
    suspended_reason: CompileReasons,
}

impl<F: CompilerFeat> Default for CompileState<F> {
    fn default() -> Self {
        Self {
            compiled_at: 0,
            watch_snap: OnceLock::new(),
            doc: None,
            success_doc: None,
            suspended_reason: CompileReasons::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct InputState {
    inputs: Arc<LazyHash<TypstDict>>,
    /// The used entry.
    entry: EntryState,
}

#[derive(Clone)]
struct CompileStateHistory<F: CompilerFeat> {
    /// The used input.
    input: InputState,
    /// The state.
    state: CompileState<F>,
}

impl<F: CompilerFeat> Default for CompileStateHistory<F> {
    fn default() -> Self {
        Self {
            input: Default::default(),
            state: Default::default(),
        }
    }
}

/// The compiler actor.
pub struct CompileServerActor<F: CompilerFeat> {
    /// The underlying universe.
    pub verse: CompilerUniverse<F>,
    /// The compilation handle.
    pub compile_handle: Arc<dyn CompilationHandle<F>>,
    /// Whether to enable file system watching.
    pub enable_watch: bool,

    /// The current logical tick.
    logical_tick: usize,
    /// Last logical tick when invalidation is caused by shadow update.
    dirty_shadow_logical_tick: usize,
    /// The latest compilation state.
    latest: CompileState<F>,
    /// The latest file changes at (revision).
    latest_file_changes_at: usize,
    /// The compilation state history.
    history: Vec<CompileStateHistory<F>>,
    /// Estimated latest set of shadow files.
    estimated_shadow_files: HashSet<Arc<Path>>,
    /// feature set for compile_once mode.
    once_feature_set: Arc<FeatureSet>,
    /// Shared feature set for watch mode.
    watch_feature_set: Arc<FeatureSet>,

    /// Channel for sending interrupts to the compiler actor.
    intr_tx: mpsc::UnboundedSender<Interrupt<F>>,
    /// Channel for receiving interrupts from the compiler actor.
    intr_rx: mpsc::UnboundedReceiver<Interrupt<F>>,
    /// Shared cache evict task.
    cache: CacheTask,

    suspended: bool,
    compiling: bool,
    committed_revision: usize,
}

impl<F: CompilerFeat + Send + Sync + 'static> CompileServerActor<F> {
    /// Create a new compiler actor with options
    pub fn new_with(
        verse: CompilerUniverse<F>,
        intr_tx: mpsc::UnboundedSender<Interrupt<F>>,
        intr_rx: mpsc::UnboundedReceiver<Interrupt<F>>,
        CompileServerOpts {
            compile_handle,
            feature_set,
            cache: cache_evict,
        }: CompileServerOpts<F>,
    ) -> Self {
        let entry = verse.entry_state();

        Self {
            verse,

            logical_tick: 1,
            compile_handle,
            enable_watch: false,
            dirty_shadow_logical_tick: 0,
            latest_file_changes_at: 0,

            estimated_shadow_files: Default::default(),
            latest: CompileState::default(),
            history: vec![],
            once_feature_set: Arc::new(feature_set.clone()),
            watch_feature_set: Arc::new(
                feature_set.configure(&WITH_COMPILING_STATUS_FEATURE, true),
            ),

            intr_tx,
            intr_rx,
            cache: cache_evict,

            suspended: entry.is_inactive(),
            compiling: false,
            committed_revision: 0,
        }
    }

    pub fn with_watch(mut self, watch: bool) -> Self {
        self.enable_watch = watch;
        self
    }

    fn make_env(&self, feature_set: Arc<FeatureSet>) -> CompileEnv {
        CompileEnv::default().configure_shared(feature_set)
    }

    /// Launches the compiler actor.
    pub async fn run(mut self) -> bool {
        if !self.enable_watch {
            let artifact = self.compile_once().await;
            return artifact.doc.is_ok();
        }

        let (dep_tx, dep_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut curr_reads = vec![];

        log::debug!("CompileServerActor: initialized");

        // Trigger the first compilation (if active)
        self.run_compile(reason_by_entry_change(), &mut curr_reads, false);

        // Spawn file system watcher.
        let fs_tx = self.intr_tx.clone();
        tokio::spawn(watch_deps(dep_rx, move |event| {
            log_send_error("fs_event", fs_tx.send(Interrupt::Fs(event)));
        }));

        'event_loop: while let Some(mut event) = self.intr_rx.recv().await {
            let mut comp_reason = no_reason();

            'accumulate: loop {
                // Warp the logical clock by one.
                self.logical_tick += 1;

                // If settle, stop the actor.
                if let Interrupt::Settle(e) = event {
                    log::info!("CompileServerActor: requested stop");
                    e.send(()).ok();
                    break 'event_loop;
                }

                if let Interrupt::CurrentRead(event) = event {
                    curr_reads.push(event);
                } else {
                    comp_reason.merge(self.process(event, |res: CompilerResponse| match res {
                        CompilerResponse::Notify(msg) => {
                            log_send_error("compile_deps", dep_tx.send(msg));
                        }
                    }));
                }

                // Try to accumulate more events.
                match self.intr_rx.try_recv() {
                    Ok(new_event) => event = new_event,
                    _ => break 'accumulate,
                }
            }

            // Either we have a reason to compile or we have events that want to have any
            // compilation.
            if comp_reason.any() || !curr_reads.is_empty() {
                self.run_compile(comp_reason, &mut curr_reads, false);
            }
        }

        log_send_error("settle_notify", dep_tx.send(NotifyMessage::Settle));
        log::info!("CompileServerActor: exited");
        true
    }

    fn snapshot(&self, is_once: bool, reason: CompileReasons) -> CompileSnapshot<F> {
        let world = self.verse.snapshot();
        let env = self.make_env(if is_once {
            self.once_feature_set.clone()
        } else {
            self.watch_feature_set.clone()
        });
        CompileSnapshot {
            world: Arc::new(world.clone()),
            env: env.clone(),
            flags: ExportSignal {
                by_entry_update: reason.by_entry_update,
                by_mem_events: reason.by_memory_events,
                by_fs_events: reason.by_fs_events,
            },
            doc_state: Arc::new(OnceLock::new()),
            compiled_doc: Arc::new(Mutex::new(None)),
            success_doc: self.latest.success_doc.clone(),
        }
    }

    /// Compile the document once.
    pub async fn compile_once(&mut self) -> CompiledArtifact<F> {
        self.run_compile(reason_by_entry_change(), &mut vec![], true)
            .unwrap()
    }

    /// Compile the document once.
    fn run_compile(
        &mut self,
        reason: CompileReasons,
        curr_reads: &mut Vec<oneshot::Sender<SucceededArtifact<F>>>,
        is_once: bool,
    ) -> Option<CompiledArtifact<F>> {
        self.latest.suspended_reason.merge(reason);
        let reason = std::mem::take(&mut self.latest.suspended_reason);
        let start = reflexo::time::now();

        let compiling = if is_once {
            self.snapshot(true, reason)
        } else {
            if reason.any() {
                self.latest.watch_snap = OnceLock::new();
            }
            let compiling = self.snapshot(false, reason);
            self.latest.watch_snap.get_or_init(|| compiling).clone()
        };

        if self.suspended {
            self.latest.suspended_reason.merge(reason);

            for reader in curr_reads.drain(..) {
                let _ = reader.send(SucceededArtifact::Suspend(compiling.clone()));
            }
            return None;
        }

        if self.compiling {
            self.latest.suspended_reason.merge(reason);
            return None;
        }

        self.compiling = true;

        let h = self.compile_handle.clone();
        let curr_reads = std::mem::take(curr_reads);

        let compile = move || {
            let handle = tokio::runtime::Handle::current();
            let h_before = h.clone();
            let compiled = compiling.compile_with(
                move |compiling| {
                    // todo unwrap main id
                    h_before.status(
                        compiling.world.revision().get(),
                        CompileReport::Stage(
                            compiling.world.main_id().unwrap(),
                            "compiling",
                            start,
                        ),
                    );
                },
                move |compiled| {
                    // todo unwrap main id
                    let id = compiled.world.main_id().unwrap();

                    // Set the runtime handle if it is not set.
                    let _enter = if tokio::runtime::Handle::try_current().is_err() {
                        Some(handle.enter())
                    } else {
                        None
                    };

                    let elapsed = start.elapsed().unwrap_or_default();

                    // log::trace!("CompileServerActor: compile reason: {:?}", compiled.signal);

                    let rep = match &compiled.doc {
                        Ok(..) => {
                            CompileReport::CompileSuccess(id, compiled.warnings.clone(), elapsed)
                        }
                        Err(err) => CompileReport::CompileError(id, err.clone(), elapsed),
                    };

                    let _ = ConsoleDiagReporter::default().export(
                        compiled.world.deref(),
                        Arc::new((compiled.env.features.clone(), rep.clone())),
                    );

                    // todo: we need to check revision for really concurrent compilation
                    h.notify_compile(compiled, rep);
                },
            );

            for reader in curr_reads {
                let _ = reader.send(SucceededArtifact::Compiled(compiled.clone()));
            }

            compiled
        };

        if is_once {
            Some(compile())
        } else {
            let intr_tx = self.intr_tx.clone();
            tokio::task::spawn_blocking(move || {
                log_send_error("compiled", intr_tx.send(Interrupt::Compiled(compile())));
            });

            None
        }
    }

    fn process_compile(&mut self, artifact: CompiledArtifact<F>, send: impl Fn(CompilerResponse)) {
        self.compiling = false;

        let w = &artifact.world;

        let compiled_revision = w.revision().get();
        if self.committed_revision >= compiled_revision {
            return;
        }

        let doc = artifact.doc.ok();

        // Update state.
        self.committed_revision = compiled_revision;
        self.latest.compiled_at = compiled_revision;
        self.latest.doc.clone_from(&doc);
        if doc.is_some() {
            self.latest.success_doc.clone_from(&self.latest.doc);
        }

        // Notify the new file dependencies.
        let mut deps = vec![];
        artifact
            .world
            .iter_dependencies(&mut |dep| deps.push(dep.clone()));
        send(CompilerResponse::Notify(NotifyMessage::SyncDependency(
            deps,
        )));

        // Trigger an evict task.
        self.cache.evict();
    }

    /// Process some interrupt. Return whether it needs compilation.
    fn process(&mut self, event: Interrupt<F>, send: impl Fn(CompilerResponse)) -> CompileReasons {
        use CompilerResponse::*;

        let reason = match event {
            Interrupt::Compile => {
                // Increment the revision anyway.
                self.verse.increment_revision(|_| {});

                reason_by_entry_change()
            }
            Interrupt::SnapshotRead(task) => {
                log::debug!("CompileServerActor: take snapshot");
                if self
                    .latest
                    .watch_snap
                    .get()
                    .is_some_and(|e| e.world.revision() < *self.verse.revision.read())
                {
                    log::info!("CompileServerActor: watch snap is outdated");
                    self.latest.watch_snap = OnceLock::new();
                }

                let _ = task.send(
                    // todo: suspicious no reason
                    self.latest
                        .watch_snap
                        .get_or_init(|| self.snapshot(false, no_reason()))
                        .clone(),
                );
                no_reason()
            }
            Interrupt::CurrentRead(..) => {
                unreachable!()
            }
            Interrupt::ChangeTask(change) => {
                let prev_state = self.verse.increment_revision(|verse| {
                    let prev_inputs = verse.inputs().clone();
                    if let Some(inputs) = change.inputs {
                        verse.set_inputs(inputs);
                    }

                    if let Some(entry) = change.entry.as_ref() {
                        match verse.mutate_entry(entry.clone()) {
                            Ok(entry) => Some(InputState {
                                inputs: prev_inputs,
                                entry,
                            }),
                            Err(err) => {
                                log::error!("CompileServerActor: change entry error: {err:?}");
                                None
                            }
                        }
                    } else {
                        None
                    }
                });

                // After incrementing the revision
                if let Some(entry) = change.entry {
                    self.suspended = entry.is_inactive();
                    if self.suspended {
                        log::info!("CompileServerActor: removing diag");
                        self.compile_handle
                            .status(self.verse.revision.get_mut().get(), CompileReport::Suspend);
                    }

                    self.switch_state(
                        InputState {
                            inputs: self.verse.inputs().clone(),
                            entry,
                        },
                        prev_state,
                    )
                } else {
                    reason_by_entry_change()
                }
            }
            Interrupt::Compiled(artifact) => {
                self.process_compile(artifact, send);
                self.process_lagged_compile()
            }
            Interrupt::Memory(event) => {
                log::debug!("CompileServerActor: memory event incoming");

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
                send(Notify(NotifyMessage::UpstreamUpdate(UpstreamUpdateEvent {
                    invalidates: files.into_iter().collect(),
                    opaque: Box::new(TaggedMemoryEvent {
                        logical_tick: self.logical_tick,
                        event,
                    }),
                })));

                no_reason()
            }
            Interrupt::Fs(mut event) => {
                log::debug!("CompileServerActor: fs event incoming {event:?}");

                let mut reason = reason_by_fs();

                // Apply file system changes.
                let dirty_tick = &mut self.dirty_shadow_logical_tick;
                self.verse.increment_revision(|verse| {
                    // Handle delayed upstream update event before applying file system changes
                    if Self::apply_delayed_memory_changes(verse, dirty_tick, &mut event).is_none() {
                        log::warn!("CompileServerActor: unknown upstream update event");

                        // Actual a delayed memory event.
                        reason = reason_by_mem();
                    }
                    verse.notify_fs_event(event)
                });

                reason
            }
            Interrupt::Settle(_) => unreachable!(),
        };

        self.check_reason_for_state(reason);
        reason
    }

    /// Process reason after each compilation.
    fn process_lagged_compile(&mut self) -> CompileReasons {
        // The reason which is kept but not used.
        std::mem::take(&mut self.latest.suspended_reason)
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
                            log::error!("CompileServerActor: read memory file at {p:?}: {err}");
                            continue;
                        }
                    };

                    let _ = verse.map_shadow(&p, insert_file);
                }
            }
        }
    }

    fn check_reason_for_state(&mut self, reason: CompileReasons) {
        if reason.file_changed() {
            self.latest_file_changes_at = self.verse.revision.get_mut().get();
        }
    }

    /// Switches the watch state and document state.
    fn switch_state(
        &mut self,
        state: InputState,
        prev_state: Option<InputState>,
    ) -> CompileReasons {
        if Some(&state) == prev_state.as_ref() {
            return no_reason();
        }

        let mut history = None;
        self.history.retain_mut(|h| {
            if h.input == state {
                history = Some(std::mem::take(h));
                false
            } else {
                true
            }
        });

        if let Some(prev_state) = prev_state {
            self.history.push(CompileStateHistory {
                input: prev_state,
                state: std::mem::take(&mut self.latest),
            });
            self.history
                .sort_by(|a, b| a.state.compiled_at.cmp(&b.state.compiled_at));

            // Only keeps the latest history, because we have race condition...
            const NUM_OF_HISTORY: usize = 1;
            if self.history.len() > NUM_OF_HISTORY {
                self.history.drain(0..self.history.len() - NUM_OF_HISTORY);
            }
        }

        if let Some(history) = history {
            if history.state.compiled_at >= self.latest_file_changes_at {
                log::info!(
                    "CompileServerActor: restore state from history: {:?} {:?}",
                    history.input,
                    history.state.watch_snap.get().is_some()
                );
                self.latest = history.state;

                // todo: race condition
                if let Some(compiling) = self.latest.watch_snap.get() {
                    log::info!(
                        "CompileServerActor: check restore diag {}",
                        compiling.compiled_doc.lock().as_ref().is_some()
                    );
                    if let Some(compiled) = compiling.compiled_doc.lock().as_ref() {
                        log::info!("CompileServerActor: restore diag");

                        let id = compiled.world.main_id().unwrap();
                        let elapsed = Default::default();

                        let rep = match &compiled.doc {
                            Ok(..) => CompileReport::CompileSuccess(
                                id,
                                compiled.warnings.clone(),
                                elapsed,
                            ),
                            Err(err) => CompileReport::CompileError(id, err.clone(), elapsed),
                        };

                        self.compile_handle.restore_compile(compiled, rep);

                        // Restore the reason.
                        return std::mem::take(&mut self.latest.suspended_reason);
                    }
                }
            }
        }

        self.latest = CompileState::default();
        reason_by_entry_change()
    }
}

#[inline]
fn log_send_error<T>(chan: &'static str, res: Result<(), mpsc::error::SendError<T>>) -> bool {
    res.map_err(|err| log::warn!("CompileServerActor: send to {chan} error: {err}"))
        .is_ok()
}
