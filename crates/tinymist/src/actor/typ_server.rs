//! The [`CompileServerActor`] implementation borrowed from typst.ts.
//!
//! Please check `tinymist::actor::typ_client` for architecture details.

use std::{collections::HashSet, path::Path, sync::Arc, thread::JoinHandle};

use tinymist_query::VersionedDocument;
use tokio::sync::{mpsc, oneshot};

use typst_ts_compiler::{
    service::{
        features::{FeatureSet, WITH_COMPILING_STATUS_FEATURE},
        watch_deps, CompileEnv, CompileReporter, Compiler, ConsoleDiagReporter,
    },
    vfs::notify::{FilesystemEvent, MemoryEvent, NotifyMessage},
    world::{CompilerFeat, CompilerUniverse, CompilerWorld},
    ShadowApi, WorldDeps,
};
use typst_ts_core::{config::compiler::EntryState, TypstDocument};

/// A task that can be sent to the context (compiler thread)
///
/// The internal function will be dereferenced and called on the context.
type BorrowTask<Ctx> = Box<dyn FnOnce(&mut Ctx) + Send + 'static>;

pub enum Interrupt<Ctx> {
    /// Compile anyway.
    Compile,
    /// Borrow the compiler thread and run the task.
    ///
    /// See [`CompileClient<Ctx>::steal_async`] for more information.
    Task(BorrowTask<Ctx>),
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

struct SuspendState {
    suspended: bool,
    dirty: bool,
}

/// The compiler thread.
pub struct CompileServerActor<C: Compiler, F: CompilerFeat> {
    /// The underlying universe.
    pub verse: CompilerUniverse<F>,
    /// The underlying compiler.
    pub compiler: CompileReporter<C, CompilerWorld<F>>,
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

    /// Channel for sending interrupts to the compiler thread.
    intr_tx: mpsc::UnboundedSender<Interrupt<Self>>,
    /// Channel for receiving interrupts from the compiler thread.
    intr_rx: mpsc::UnboundedReceiver<Interrupt<Self>>,

    suspend_state: SuspendState,
}

impl<F: CompilerFeat + Send + 'static, C: Compiler<W = CompilerWorld<F>> + Send + 'static>
    CompileServerActor<C, F>
{
    pub fn new_with_features(
        compiler: C,
        world: CompilerUniverse<F>,
        entry: EntryState,
        feature_set: FeatureSet,
        intr_tx: mpsc::UnboundedSender<Interrupt<Self>>,
        intr_rx: mpsc::UnboundedReceiver<Interrupt<Self>>,
    ) -> Self {
        Self {
            compiler: CompileReporter::new(compiler)
                .with_generic_reporter(ConsoleDiagReporter::default()),
            verse: world,

            logical_tick: 1,
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

            suspend_state: SuspendState {
                suspended: entry.is_inactive(),
                dirty: false,
            },
        }
    }

    /// Create a new compiler thread.
    pub fn new(
        compiler: C,
        world: CompilerUniverse<F>,
        entry: EntryState,
        intr_tx: mpsc::UnboundedSender<Interrupt<Self>>,
        intr_rx: mpsc::UnboundedReceiver<Interrupt<Self>>,
    ) -> Self {
        Self::new_with_features(
            compiler,
            world,
            entry,
            FeatureSet::default(),
            intr_tx,
            intr_rx,
        )
    }

    pub fn success_doc(&self) -> Option<VersionedDocument> {
        self.latest_success_doc
            .clone()
            .map(|doc| VersionedDocument {
                version: self.logical_tick,
                document: doc,
            })
    }

    pub fn doc(&self) -> Option<VersionedDocument> {
        self.latest_doc.clone().map(|doc| VersionedDocument {
            version: self.logical_tick,
            document: doc,
        })
    }

    fn make_env(&self, feature_set: Arc<FeatureSet>) -> CompileEnv {
        CompileEnv::default().configure_shared(feature_set)
    }

    /// Run the compiler thread synchronously.
    pub fn run(self) -> bool {
        use tokio::runtime::Handle;

        if Handle::try_current().is_err() && self.enable_watch {
            log::error!("Typst compiler thread with watch enabled must be run in a tokio runtime");
            return false;
        }

        tokio::task::block_in_place(move || Handle::current().block_on(self.block_run_inner()))
    }

    /// Inner function for `run`, it launches the compiler thread and blocks
    /// until it exits.
    async fn block_run_inner(mut self) -> bool {
        if !self.enable_watch {
            let mut env = self.make_env(self.once_feature_set.clone());
            let w = self.verse.spawn();
            let compiled = self.compiler.compile(&w, &mut env);
            return compiled.is_ok();
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
            let mut env = self.make_env(self.once_feature_set.clone());
            let w = self.verse.spawn();
            self.compiler.compile(&w, &mut env).ok();
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

        // Spawn file system watcher.
        // todo: don't compile if no entry
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
                    if matches!(event, Interrupt::Task(_)) && need_compile {
                        self.compile(&compiler_ack);
                        need_compile = false;
                    }

                    need_compile |= self.process(event, &compiler_ack);

                    // Try to accumulate more events.
                    match self.intr_rx.try_recv() {
                        Ok(new_event) => event = new_event,
                        _ => break 'accumulate,
                    }
                }

                if need_compile {
                    self.compile(&compiler_ack);
                }
            }

            settle_notify();
            log::info!("CompileServerActor: exited");
        });

        // Return the thread handle.
        Some(compile_thread.unwrap())
    }

    pub fn change_entry(&mut self, entry: EntryState) {
        self.suspend_state.suspended = entry.is_inactive();
        if !self.suspend_state.suspended && self.suspend_state.dirty {
            self.intr_tx.send(Interrupt::Compile).ok();
        }

        // Reset the document state.
        self.latest_doc = None;
        self.latest_success_doc = None;
    }

    /// Compile the document.
    fn compile(&mut self, send: impl Fn(CompilerResponse)) {
        use CompilerResponse::*;

        if self.suspend_state.suspended {
            self.suspend_state.dirty = true;
            return;
        }

        let w = self.verse.spawn();

        // Compile the document.
        let mut env = self.make_env(self.watch_feature_set.clone());
        self.latest_doc = self.compiler.compile(&w, &mut env).ok();
        if self.latest_doc.is_some() {
            self.latest_success_doc.clone_from(&self.latest_doc);
        }

        // Evict compilation cache.
        let evict_start = std::time::Instant::now();
        comemo::evict(30);
        let elapsed = evict_start.elapsed();
        log::info!("CompileServerActor: evict compilation cache in {elapsed:?}",);

        // Notify the new file dependencies.
        let mut deps = vec![];
        w.iter_dependencies(&mut |dep| deps.push(dep.clone()));
        send(Notify(NotifyMessage::SyncDependency(deps)));
    }

    /// Process some interrupt. Return whether it needs compilation.
    fn process(&mut self, event: Interrupt<Self>, send: impl Fn(CompilerResponse)) -> bool {
        use CompilerResponse::*;

        match event {
            Interrupt::Compile => true,
            Interrupt::Task(task) => {
                log::debug!("CompileServerActor: execute task");
                task(self);
                false
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
                    self.apply_memory_changes(event);
                    return true;
                }

                // Otherwise, send upstream update event.
                // Also, record the logical tick when shadow is dirty.
                self.dirty_shadow_logical_tick = self.logical_tick;
                send(Notify(NotifyMessage::UpstreamUpdate(
                    typst_ts_compiler::vfs::notify::UpstreamUpdateEvent {
                        invalidates: files.into_iter().collect(),
                        opaque: Box::new(TaggedMemoryEvent {
                            logical_tick: self.logical_tick,
                            event,
                        }),
                    },
                )));

                false
            }
            Interrupt::Fs(mut event) => {
                log::debug!("CompileServerActor: fs event incoming {event:?}");

                // Handle delayed upstream update event before applying file system changes
                if self.apply_delayed_memory_changes(&mut event).is_none() {
                    log::warn!("CompileServerActor: unknown upstream update event");
                }

                // Apply file system changes.
                self.verse.notify_fs_event(event);

                true
            }
            Interrupt::Settle(_) => unreachable!(),
        }
    }

    /// Apply delayed memory changes to underlying compiler.
    fn apply_delayed_memory_changes(&mut self, event: &mut FilesystemEvent) -> Option<()> {
        // Handle delayed upstream update event before applying file system changes
        if let FilesystemEvent::UpstreamUpdate { upstream_event, .. } = event {
            let event = upstream_event.take()?.opaque;
            let TaggedMemoryEvent {
                logical_tick,
                event,
            } = *event.downcast().ok()?;

            // Recovery from dirty shadow state.
            if logical_tick == self.dirty_shadow_logical_tick {
                self.dirty_shadow_logical_tick = 0;
            }

            self.apply_memory_changes(event);
        }

        Some(())
    }

    /// Apply memory changes to underlying compiler.
    fn apply_memory_changes(&mut self, event: MemoryEvent) {
        if matches!(event, MemoryEvent::Sync(..)) {
            self.verse.reset_shadow();
        }
        match event {
            MemoryEvent::Update(event) | MemoryEvent::Sync(event) => {
                for removes in event.removes {
                    let _ = self.verse.unmap_shadow(&removes);
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

                    let _ = self.verse.map_shadow(&p, insert_file);
                }
            }
        }
    }
}

impl<C: Compiler, F: CompilerFeat> CompileServerActor<C, F> {
    pub fn with_watch(mut self, enable_watch: bool) -> Self {
        self.enable_watch = enable_watch;
        self
    }

    pub fn intr_tx(&self) -> mpsc::UnboundedSender<Interrupt<Self>> {
        self.intr_tx.clone()
    }

    pub fn document(&self) -> Option<Arc<TypstDocument>> {
        self.latest_doc.clone()
    }
}

#[inline]
fn log_send_error<T>(chan: &'static str, res: Result<(), mpsc::error::SendError<T>>) -> bool {
    res.map_err(|err| log::warn!("CompileServerActor: send to {chan} error: {err}"))
        .is_ok()
}
