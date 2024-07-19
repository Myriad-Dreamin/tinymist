//! The [`CompileServerActor`] implementation borrowed from typst.ts.
//!
//! Please check `tinymist::actor::typ_client` for architecture details.

use std::{
    collections::HashSet,
    ops::Deref,
    path::Path,
    sync::{Arc, OnceLock},
};

use once_cell::sync::OnceCell;
use tokio::sync::{mpsc, oneshot};

use typst::{diag::SourceResult, util::Deferred};
use typst_ts_compiler::{
    features::{FeatureSet, WITH_COMPILING_STATUS_FEATURE},
    vfs::notify::{FilesystemEvent, MemoryEvent, NotifyMessage, UpstreamUpdateEvent},
    watch_deps,
    world::{CompilerFeat, CompilerUniverse, CompilerWorld},
    CompileEnv, CompileReport, Compiler, ConsoleDiagReporter, EntryReader, Revising, TaskInputs,
    WorldDeps,
};
use typst_ts_core::{exporter_builtins::GroupExporter, Exporter, GenericExporter, TypstDocument};

use crate::task::CacheTask;

type CompileRawResult = Deferred<(SourceResult<Arc<TypstDocument>>, CompileEnv)>;
type DocState = once_cell::sync::OnceCell<CompileRawResult>;

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
    doc_state: Arc<DocState>,
    /// The last successfully compiled document.
    pub success_doc: Option<Arc<TypstDocument>>,
}

impl<F: CompilerFeat + 'static> CompileSnapshot<F> {
    fn start(&self) -> &CompileRawResult {
        self.doc_state.get_or_init(|| {
            let w = self.world.clone();
            let mut env = self.env.clone();
            Deferred::new(move || {
                let w = w.as_ref();
                let mut c = std::marker::PhantomData;
                let res = c.ensure_main(w).and_then(|_| c.compile(w, &mut env));
                (res, env)
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
        self.doc_state = Arc::new(OnceCell::new());

        self
    }

    pub fn compile(&self) -> CompiledArtifact<F> {
        let (doc, env) = self.start().wait().clone();
        CompiledArtifact {
            signal: self.flags,
            world: self.world.clone(),
            env,
            doc,
            success_doc: self.success_doc.clone(),
        }
    }
}

impl<F: CompilerFeat> Clone for CompileSnapshot<F> {
    fn clone(&self) -> Self {
        Self {
            flags: self.flags,
            env: self.env.clone(),
            world: self.world.clone(),
            doc_state: self.doc_state.clone(),
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
    pub doc: SourceResult<Arc<TypstDocument>>,
    success_doc: Option<Arc<TypstDocument>>,
}

impl<F: CompilerFeat> Clone for CompiledArtifact<F> {
    fn clone(&self) -> Self {
        Self {
            signal: self.signal,
            world: self.world.clone(),
            env: self.env.clone(),
            doc: self.doc.clone(),
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

// pub type NopCompilationHandle<T> = std::marker::PhantomData<fn(T)>;

pub trait CompilationHandle<F: CompilerFeat>: Send + Sync + 'static {
    fn status(&self, revision: usize, rep: CompileReport);
    fn notify_compile(&self, res: &CompiledArtifact<F>, rep: CompileReport);
}

impl<F: CompilerFeat + Send + Sync + 'static> CompilationHandle<F>
    for std::marker::PhantomData<fn(F)>
{
    fn status(&self, _revision: usize, _: CompileReport) {}
    fn notify_compile(&self, _: &CompiledArtifact<F>, _: CompileReport) {}
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
    /// Request compiler to snapshot the current state.
    Snapshot(oneshot::Sender<CompileSnapshot<F>>),
    /// Request compiler to get latest succeeded artifact.
    SucceededArtifact(oneshot::Sender<SucceededArtifact<F>>),
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
    fn see(&mut self, reason: CompileReasons) {
        self.by_memory_events |= reason.by_memory_events;
        self.by_fs_events |= reason.by_fs_events;
        self.by_entry_update |= reason.by_entry_update;
    }

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
    pub exporter: GroupExporter<CompileSnapshot<F>>,
    pub feature_set: FeatureSet,
    pub cache: CacheTask,
}

impl<F: CompilerFeat + Send + Sync + 'static> Default for CompileServerOpts<F> {
    fn default() -> Self {
        Self {
            exporter: GroupExporter::new(vec![]),
            feature_set: FeatureSet::default(),
            cache: CacheTask::new(Default::default()),
        }
    }
}

/// The compiler actor.
pub struct CompileServerActor<F: CompilerFeat> {
    /// The underlying universe.
    pub verse: CompilerUniverse<F>,
    /// The exporter for the compiled document.
    pub exporter: GroupExporter<CompileSnapshot<F>>,
    /// The compilation handle.
    pub watch_handle: Arc<dyn CompilationHandle<F>>,
    /// Whether to enable file system watching.
    pub enable_watch: bool,

    /// The current logical tick.
    logical_tick: usize,
    /// Last logical tick when invalidation is caused by shadow update.
    dirty_shadow_logical_tick: usize,

    /// Estimated latest set of shadow files.
    estimated_shadow_files: HashSet<Arc<Path>>,
    /// The latest compiled document.
    pub(crate) latest_doc: Option<Arc<TypstDocument>>,
    /// The latest successly compiled document.
    latest_success_doc: Option<Arc<TypstDocument>>,
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

    watch_snap: OnceLock<CompileSnapshot<F>>,
    suspended: bool,
    compiling: bool,
    suspended_reason: CompileReasons,
    committed_revision: usize,
}

impl<F: CompilerFeat + Send + Sync + 'static> CompileServerActor<F> {
    /// Create a new compiler actor with options
    pub fn new_with(
        verse: CompilerUniverse<F>,
        intr_tx: mpsc::UnboundedSender<Interrupt<F>>,
        intr_rx: mpsc::UnboundedReceiver<Interrupt<F>>,
        CompileServerOpts {
            exporter,
            feature_set,
            cache: cache_evict,
        }: CompileServerOpts<F>,
    ) -> Self {
        let entry = verse.entry_state();

        Self {
            exporter,
            verse,

            logical_tick: 1,
            watch_handle: Arc::new(std::marker::PhantomData),
            enable_watch: false,
            dirty_shadow_logical_tick: 0,

            estimated_shadow_files: Default::default(),
            latest_doc: None,
            latest_success_doc: None,
            once_feature_set: Arc::new(feature_set.clone()),
            watch_feature_set: Arc::new(
                feature_set.configure(&WITH_COMPILING_STATUS_FEATURE, true),
            ),

            intr_tx,
            intr_rx,
            cache: cache_evict,

            watch_snap: OnceLock::new(),
            suspended: entry.is_inactive(),
            compiling: false,
            suspended_reason: no_reason(),
            committed_revision: 0,
        }
    }

    /// Create a new compiler actor.
    pub fn new(
        verse: CompilerUniverse<F>,
        intr_tx: mpsc::UnboundedSender<Interrupt<F>>,
        intr_rx: mpsc::UnboundedReceiver<Interrupt<F>>,
    ) -> Self {
        Self::new_with(verse, intr_tx, intr_rx, CompileServerOpts::default())
    }

    pub fn with_watch(mut self, watch: Option<Arc<dyn CompilationHandle<F>>>) -> Self {
        self.enable_watch = watch.is_some();
        match watch {
            Some(watch) => self.watch_handle = watch,
            None => self.watch_handle = Arc::new(std::marker::PhantomData),
        }
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
        let mut snapshot_events = vec![];

        log::debug!("CompileServerActor: initialized");

        // Trigger the first compilation (if active)
        self.watch_compile(reason_by_entry_change(), &mut snapshot_events);

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

                if let Interrupt::SucceededArtifact(event) = event {
                    snapshot_events.push(event);
                } else {
                    comp_reason.see(self.process(event, |res: CompilerResponse| match res {
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
            if comp_reason.any() || !snapshot_events.is_empty() {
                self.watch_compile(comp_reason, &mut snapshot_events);
            }
        }

        log_send_error("settle_notify", dep_tx.send(NotifyMessage::Settle));
        log::info!("CompileServerActor: exited");
        true
    }

    fn snapshot(&self, is_once: bool, reason: CompileReasons) -> CompileSnapshot<F> {
        let world = self.verse.snapshot();
        let mut env = self.make_env(if is_once {
            self.once_feature_set.clone()
        } else {
            self.watch_feature_set.clone()
        });
        if env.tracer.is_none() {
            env.tracer = Some(Default::default());
        }
        CompileSnapshot {
            world: Arc::new(world.clone()),
            env: env.clone(),
            flags: ExportSignal {
                by_entry_update: reason.by_entry_update,
                by_mem_events: reason.by_memory_events,
                by_fs_events: reason.by_fs_events,
            },
            doc_state: Arc::new(OnceCell::new()),
            success_doc: self.latest_success_doc.clone(),
        }
    }

    /// Compile the document once.
    pub async fn compile_once(&mut self) -> CompiledArtifact<F> {
        let e = Arc::new(self.snapshot(true, reason_by_fs()));
        let err = self.exporter.export(e.world.deref(), e.clone());
        if let Err(err) = err {
            // todo: ExportError
            log::error!("CompileServerActor: export error: {err:?}");
        }

        e.compile()
    }

    /// Watch and compile the document once.
    fn watch_compile(
        &mut self,
        reason: CompileReasons,
        snapshot_events: &mut Vec<oneshot::Sender<SucceededArtifact<F>>>,
    ) {
        self.suspended_reason.see(reason);
        let reason = std::mem::take(&mut self.suspended_reason);
        let start = reflexo::time::now();

        let compiling = self.snapshot(false, reason);
        self.watch_snap = OnceLock::new();
        self.watch_snap.get_or_init(|| compiling.clone());

        if self.suspended {
            self.suspended_reason.see(reason);

            for task in snapshot_events.drain(..) {
                let _ = task.send(SucceededArtifact::Suspend(compiling.clone()));
            }
            return;
        }

        if self.compiling {
            self.suspended_reason.see(reason);
            return;
        }

        self.compiling = true;

        let h = self.watch_handle.clone();
        let snapshot_events = std::mem::take(snapshot_events);

        // todo unwrap main id
        let id = compiling.world.main_id().unwrap();
        let revision = compiling.world.revision().get();

        h.status(revision, CompileReport::Stage(id, "compiling", start));

        let compile = move || {
            let compiled = compiling.compile();

            for task in snapshot_events {
                let _ = task.send(SucceededArtifact::Compiled(compiled.clone()));
            }

            let elapsed = start.elapsed().unwrap_or_default();
            let rep;
            match &compiled.doc {
                Ok(..) => {
                    let warnings = compiled.env.tracer.as_ref().unwrap().clone().warnings();
                    if warnings.is_empty() {
                        rep = CompileReport::CompileSuccess(id, warnings, elapsed);
                    } else {
                        rep = CompileReport::CompileWarning(id, warnings, elapsed);
                    }
                }
                Err(err) => {
                    rep = CompileReport::CompileError(id, err.clone(), elapsed);
                }
            };

            let _ = ConsoleDiagReporter::default().export(
                compiled.world.deref(),
                Arc::new((compiled.env.features.clone(), rep.clone())),
            );

            // todo: we need to check revision for really concurrent compilation
            h.notify_compile(&compiled, rep);

            compiled
        };

        let intr_tx = self.intr_tx.clone();
        tokio::task::spawn_blocking(move || {
            log_send_error("compiled", intr_tx.send(Interrupt::Compiled(compile())));
        });
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
        self.latest_doc.clone_from(&doc);
        if doc.is_some() {
            self.latest_success_doc.clone_from(&self.latest_doc);
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

        match event {
            Interrupt::Compile => reason_by_entry_change(),
            Interrupt::Snapshot(task) => {
                log::debug!("CompileServerActor: take snapshot");
                if self
                    .watch_snap
                    .get()
                    .is_some_and(|e| e.world.revision() < *self.verse.revision.read())
                {
                    self.watch_snap = OnceLock::new();
                }

                let _ = task.send(
                    self.watch_snap
                        .get_or_init(|| self.snapshot(false, no_reason()))
                        .clone(),
                );
                no_reason()
            }
            Interrupt::SucceededArtifact(..) => {
                unreachable!()
            }
            Interrupt::ChangeTask(change) => {
                self.verse.increment_revision(|verse| {
                    if let Some(inputs) = change.inputs {
                        verse.set_inputs(inputs);
                    }

                    if let Some(entry) = change.entry.clone() {
                        let res = verse.mutate_entry(entry);
                        if let Err(err) = res {
                            log::error!("CompileServerActor: change entry error: {err:?}");
                        }
                    }
                });

                // After incrementing the revision
                if let Some(entry) = change.entry {
                    self.suspended = entry.is_inactive();
                    if self.suspended {
                        log::info!("CompileServerActor: removing diag");
                        self.watch_handle
                            .status(self.verse.revision.get_mut().get(), CompileReport::Suspend);
                    }

                    // Reset the document state.
                    self.latest_doc = None;
                    self.latest_success_doc = None;
                }

                reason_by_entry_change()
            }
            Interrupt::Compiled(artifact) => {
                self.process_compile(artifact, send);
                no_reason()
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
                            log::error!(
                                "CompileServerActor: read memory file at {}: {}",
                                p.display(),
                                err,
                            );
                            continue;
                        }
                    };

                    let _ = verse.map_shadow(&p, insert_file);
                }
            }
        }
    }
}

#[inline]
fn log_send_error<T>(chan: &'static str, res: Result<(), mpsc::error::SendError<T>>) -> bool {
    res.map_err(|err| log::warn!("CompileServerActor: send to {chan} error: {err}"))
        .is_ok()
}
