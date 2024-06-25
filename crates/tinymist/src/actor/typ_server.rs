//! The [`CompileServerActor`] implementation borrowed from typst.ts.
//!
//! Please check `tinymist::actor::typ_client` for architecture details.

use std::{
    collections::HashSet,
    ops::Deref,
    path::Path,
    sync::{Arc, OnceLock},
    thread::JoinHandle,
};

use tokio::sync::{mpsc, oneshot};

use typst::{diag::SourceResult, util::Deferred};
use typst_ts_compiler::{
    features::{FeatureSet, WITH_COMPILING_STATUS_FEATURE},
    vfs::notify::{FilesystemEvent, MemoryEvent, NotifyMessage, UpstreamUpdateEvent},
    watch_deps,
    world::{CompilerFeat, CompilerUniverse, CompilerWorld},
    CompileEnv, CompileReport, CompileReporter, Compiler, ConsoleDiagReporter, EntryReader,
    PureCompiler, Revising, TaskInputs, WorldDeps,
};
use typst_ts_core::{
    config::compiler::EntryState, exporter_builtins::GroupExporter, Exporter, QueryRef,
    TypstDocument,
};

type UsingCompiler<F> = CompileReporter<PureCompiler<CompilerWorld<F>>, CompilerWorld<F>>;
type CompileRawResult = Deferred<(SourceResult<Arc<TypstDocument>>, CompileEnv)>;
type DocState<F> = QueryRef<CompileRawResult, (), (UsingCompiler<F>, CompileEnv)>;

pub struct CompileSnapshot<F: CompilerFeat> {
    /// The compiler-thread local logical tick when the snapshot is taken.
    pub compile_tick: usize,
    /// Using env
    pub env: CompileEnv,
    /// Using world
    pub world: Arc<CompilerWorld<F>>,
    /// Compiling the document.
    doc_state: Arc<DocState<F>>,
    /// The last successfully compiled document.
    pub success_doc: Option<Arc<TypstDocument>>,
}

impl<F: CompilerFeat + 'static> CompileSnapshot<F> {
    pub fn start(&self) -> &CompileRawResult {
        let res = self.doc_state.compute_with_context(|(mut c, mut env)| {
            let w = self.world.clone();
            Ok(Deferred::new(move || {
                let res = c.compile(&w, &mut env);
                (res, env)
            }))
        });
        res.ok().unwrap()
    }

    pub fn doc(&self) -> SourceResult<Arc<TypstDocument>> {
        self.start().wait().0.clone()
    }

    pub fn compile(&self) -> CompiledArtifact<F> {
        let (doc, env) = self.start().wait().clone();
        CompiledArtifact {
            world: self.world.clone(),
            compile_tick: self.compile_tick,
            doc,
            env,
            success_doc: self.success_doc.clone(),
        }
    }
}

impl<F: CompilerFeat> Clone for CompileSnapshot<F> {
    fn clone(&self) -> Self {
        Self {
            compile_tick: self.compile_tick,
            env: self.env.clone(),
            world: self.world.clone(),
            doc_state: self.doc_state.clone(),
            success_doc: self.success_doc.clone(),
        }
    }
}

#[derive(Clone)]
pub struct CompiledArtifact<F: CompilerFeat> {
    pub world: Arc<CompilerWorld<F>>,
    pub compile_tick: usize,
    pub doc: SourceResult<Arc<TypstDocument>>,
    /// Used env
    pub env: CompileEnv,
    pub success_doc: Option<Arc<TypstDocument>>,
}

// pub type NopCompilationHandle<T> = std::marker::PhantomData<fn(T)>;

#[cfg(feature = "stable-server")]
const COMPILE_CONCURRENCY: usize = 0;
#[cfg(not(feature = "stable-server"))]
const COMPILE_CONCURRENCY: usize = 1;

pub trait CompilationHandle<F: CompilerFeat>: Send + Sync + 'static {
    fn status(&self, rep: CompileReport);
    fn notify_compile(&self, res: &CompiledArtifact<F>, rep: CompileReport);
}

impl<F: CompilerFeat + Send + Sync + 'static> CompilationHandle<F>
    for std::marker::PhantomData<fn(F)>
{
    fn status(&self, _: CompileReport) {}
    fn notify_compile(&self, _: &CompiledArtifact<F>, _: CompileReport) {}
}

pub enum Interrupt<F: CompilerFeat> {
    /// Compiled from computing thread.
    Compiled(CompiledArtifact<F>),
    /// Change the watching entry.
    ChangeTask(TaskInputs),
    /// Request compiler to snapshot the current state.
    Snapshot(oneshot::Sender<CompileSnapshot<F>>),
    /// Memory file changes.
    Memory(MemoryEvent),
    /// File system event.
    Fs(FilesystemEvent),
    /// Request compiler to stop.
    Settle(oneshot::Sender<()>),
}

/// Responses from the compiler thread.
enum CompilerResponse {
    /// Response to the file watcher
    Notify(NotifyMessage),
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
    pub compile_concurrency: usize,
}

impl<F: CompilerFeat + Send + Sync + 'static> Default for CompileServerOpts<F> {
    fn default() -> Self {
        Self {
            exporter: GroupExporter::new(vec![]),
            feature_set: FeatureSet::default(),
            compile_concurrency: COMPILE_CONCURRENCY,
        }
    }
}

/// The compiler actor.
pub struct CompileServerActor<F: CompilerFeat> {
    /// The underlying universe.
    pub verse: CompilerUniverse<F>,
    /// The underlying compiler.
    pub compiler: CompileReporter<PureCompiler<CompilerWorld<F>>, CompilerWorld<F>>,
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

    // todo: private me
    /// Channel for sending interrupts to the compiler thread.
    pub intr_tx: mpsc::UnboundedSender<Interrupt<F>>,
    /// Channel for receiving interrupts from the compiler thread.
    intr_rx: mpsc::UnboundedReceiver<Interrupt<F>>,

    watch_snap: OnceLock<CompileSnapshot<F>>,
    suspended: bool,
    committed_revision: usize,
    compile_concurrency: usize,
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
            compile_concurrency,
        }: CompileServerOpts<F>,
    ) -> Self {
        let entry = verse.entry_state();

        Self {
            compiler: CompileReporter::new(std::marker::PhantomData)
                .with_generic_reporter(ConsoleDiagReporter::default()),
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

            watch_snap: OnceLock::new(),
            suspended: entry.is_inactive(),
            committed_revision: 0,
            compile_concurrency,
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

    /// Launches the compiler thread and blocks until it exits.
    #[allow(unused)]
    pub async fn run_and_wait(mut self) -> bool {
        if !self.enable_watch {
            let artifact = self.compile_once();
            return artifact.doc.is_ok();
        }

        if let Some(h) = self.spawn().await {
            // Note: this is blocking the current thread.
            // Note: the block safety is ensured by `run` function.
            h.join().unwrap();
        }

        true
    }

    /// Spawn the compiler thread.
    pub async fn spawn(mut self) -> Option<JoinHandle<()>> {
        if !self.enable_watch {
            self.compile_once();
            return None;
        }

        // Setup internal channel.
        let (dep_tx, dep_rx) = tokio::sync::mpsc::unbounded_channel();

        let settle_notify_tx = dep_tx.clone();
        let settle_notify = move || {
            log_send_error(
                "settle_notify",
                settle_notify_tx.send(NotifyMessage::Settle),
            )
        };

        // Wrap sender to send compiler response.
        let compiler_ack = move |res: CompilerResponse| match res {
            CompilerResponse::Notify(msg) => {
                log_send_error("compile_deps", dep_tx.send(msg));
            }
        };

        // Trigger the first compilation (if active)
        self.watch_compile(&compiler_ack);

        // Spawn file system watcher.
        let fs_tx = self.intr_tx.clone();
        tokio::spawn(watch_deps(dep_rx, move |event| {
            log_send_error("fs_event", fs_tx.send(Interrupt::Fs(event)));
        }));

        // Spawn compiler thread.
        let thread_builder = std::thread::Builder::new().name("typst-compiler".to_owned());
        let compile_thread = thread_builder.spawn(move || {
            log::debug!("CompileServerActor: initialized");

            // Wait for first events.
            'event_loop: while let Some(mut event) = self.intr_rx.blocking_recv() {
                let mut need_compile = false;

                'accumulate: loop {
                    // Warp the logical clock by one.
                    self.logical_tick += 1;

                    // If settle, stop the actor.
                    if let Interrupt::Settle(e) = event {
                        log::info!("CompileServerActor: requested stop");
                        e.send(()).ok();
                        break 'event_loop;
                    }

                    // Ensure complied before executing tasks.
                    if matches!(event, Interrupt::Snapshot(_)) && need_compile {
                        need_compile = self.watch_compile(&compiler_ack);
                    }

                    need_compile |= self.process(event, &compiler_ack);

                    // Try to accumulate more events.
                    match self.intr_rx.try_recv() {
                        Ok(new_event) => event = new_event,
                        _ => break 'accumulate,
                    }
                }

                if need_compile {
                    need_compile = self.watch_compile(&compiler_ack);
                }
                if need_compile {
                    need_compile = self.watch_compile(&compiler_ack);
                    if need_compile {
                        log::warn!("CompileServerActor: watch_compile infinite loop?");
                    }
                }
            }

            settle_notify();
            log::info!("CompileServerActor: exited");
        });

        // Return the thread handle.
        Some(compile_thread.unwrap())
    }

    fn snapshot(&self, is_once: bool) -> CompileSnapshot<F> {
        let world = self.verse.snapshot();
        let c = self.compiler.clone();
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
            compile_tick: self.logical_tick,
            doc_state: Arc::new(QueryRef::with_context((c, env))),
            success_doc: self.latest_success_doc.clone(),
        }
    }

    /// Compile the document once.
    pub fn compile_once(&mut self) -> CompiledArtifact<F> {
        let e = Arc::new(self.snapshot(true));
        let err = self.exporter.export(e.world.deref(), e.clone());
        if let Err(err) = err {
            // todo: ExportError
            log::error!("CompileServerActor: export error: {err:?}");
        }

        e.compile()
    }

    /// Watch and compile the document once.
    fn watch_compile(&mut self, send: impl Fn(CompilerResponse)) -> bool {
        if self.suspended {
            return false;
        }

        let start = reflexo::time::now();

        let compiling = self.snapshot(false);
        self.watch_snap = OnceLock::new();
        self.watch_snap.get_or_init(|| compiling.clone());

        let h = self.watch_handle.clone();
        let intr_tx = self.intr_tx.clone();

        // todo unwrap main id
        let id = compiling.world.main_id().unwrap();
        self.watch_handle
            .status(CompileReport::Stage(id, "compiling", start));

        let compile = move || {
            let compiled = compiling.compile();
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

            h.notify_compile(&compiled, rep);

            compiled
        };

        if self.compile_concurrency == 0 {
            self.processs_compile(compile(), send)
        } else {
            rayon::spawn(move || {
                log_send_error("compiled", intr_tx.send(Interrupt::Compiled(compile())));
            });
            false
        }
    }

    fn processs_compile(
        &mut self,
        artifact: CompiledArtifact<F>,
        send: impl Fn(CompilerResponse),
    ) -> bool {
        let w = &artifact.world;

        let compiled_revision = w.revision().get();
        if self.committed_revision >= compiled_revision {
            return false;
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
        rayon::spawn(move || {
            // Evict compilation cache.
            let evict_start = std::time::Instant::now();
            comemo::evict(30);
            let elapsed = evict_start.elapsed();
            log::info!("CompileServerActor: evict compilation cache in {elapsed:?}");
        });

        self.process_may_laggy_compile()
    }

    fn process_may_laggy_compile(&mut self) -> bool {
        // todo: rate limit
        false
    }

    /// Process some interrupt. Return whether it needs compilation.
    fn process(&mut self, event: Interrupt<F>, send: impl Fn(CompilerResponse)) -> bool {
        use CompilerResponse::*;

        match event {
            Interrupt::Snapshot(task) => {
                log::debug!("CompileServerActor: take snapshot");
                let _ = task.send(self.watch_snap.get_or_init(|| self.snapshot(false)).clone());
                false
            }
            Interrupt::ChangeTask(change) => {
                if let Some(entry) = change.entry.clone() {
                    self.change_entry(entry.clone());
                }

                self.verse.increment_revision(|verse| {
                    if let Some(inputs) = change.inputs {
                        verse.set_inputs(inputs);
                    }

                    if let Some(entry) = change.entry {
                        let res = verse.mutate_entry(entry);
                        if let Err(err) = res {
                            log::error!("CompileServerActor: change entry error: {err:?}");
                        }
                    }
                });

                true
            }
            Interrupt::Compiled(artifact) => self.processs_compile(artifact, send),
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
                    return true;
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

                false
            }
            Interrupt::Fs(mut event) => {
                log::debug!("CompileServerActor: fs event incoming {event:?}");

                // Apply file system changes.
                let dirty_tick = &mut self.dirty_shadow_logical_tick;
                self.verse.increment_revision(|verse| {
                    // Handle delayed upstream update event before applying file system changes
                    if Self::apply_delayed_memory_changes(verse, dirty_tick, &mut event).is_none() {
                        log::warn!("CompileServerActor: unknown upstream update event");
                    }
                    verse.notify_fs_event(event)
                });

                true
            }
            Interrupt::Settle(_) => unreachable!(),
        }
    }

    fn change_entry(&mut self, entry: EntryState) -> bool {
        self.suspended = entry.is_inactive();
        if self.suspended {
            log::info!("CompileServerActor: removing diag");
            self.watch_handle.status(CompileReport::Suspend);
        }

        // Reset the document state.
        self.latest_doc = None;
        self.latest_success_doc = None;

        !self.suspended
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
