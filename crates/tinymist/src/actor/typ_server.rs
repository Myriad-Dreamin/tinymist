//! The [`CompileServerActor`] implementation borrowed from typst.ts.
//!
//! Please check `tinymist::actor::typ_client` for architecture details.

use std::{
    collections::HashSet,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    sync::Arc,
    thread::JoinHandle,
};

use serde::Serialize;
use tinymist_query::VersionedDocument;
use tokio::sync::{mpsc, oneshot};
use typst::{
    layout::{Frame, FrameItem, Point, Position},
    syntax::{LinkedNode, Source, Span, SyntaxKind, VirtualPath},
    World,
};

use typst_ts_compiler::{
    service::{
        features::{FeatureSet, WITH_COMPILING_STATUS_FEATURE},
        watch_deps, CompileEnv, CompileReporter, Compiler, ConsoleDiagReporter, EntryManager,
    },
    vfs::notify::{FilesystemEvent, MemoryEvent, NotifyMessage},
    world::{CompilerFeat, CompilerWorld},
    ShadowApi,
};
use typst_ts_core::{
    config::compiler::EntryState,
    debug_loc::{SourceLocation, SourceSpanOffset},
    error::prelude::{map_string_err, ZResult},
    TypstDocument, TypstFileId,
};

use crate::{task::BorrowTask, utils};

pub trait EntryStateExt {
    fn is_inactive(&self) -> bool;
}

impl EntryStateExt for EntryState {
    fn is_inactive(&self) -> bool {
        matches!(
            self,
            EntryState::Detached | EntryState::Workspace { main: None, .. }
        )
    }
}

enum Interrupt<Ctx> {
    /// Compile anyway.
    Compile,
    /// Borrow the compiler thread and run the task.
    ///
    /// See [`CompileClient<Ctx>::steal`] for more information.
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
pub struct CompileServerActor<C: Compiler> {
    /// The underlying compiler.
    pub compiler: CompileReporter<C>,
    /// Whether to enable file system watching.
    pub enable_watch: bool,

    /// The current logical tick.
    logical_tick: usize,
    /// Last logical tick when invalidation is caused by shadow update.
    dirty_shadow_logical_tick: usize,

    /// Estimated latest set of shadow files.
    estimated_shadow_files: HashSet<Arc<Path>>,
    /// The latest compiled document.
    latest_doc: Option<Arc<TypstDocument>>,
    /// The latest successly compiled document.
    latest_success_doc: Option<Arc<TypstDocument>>,
    /// feature set for compile_once mode.
    once_feature_set: Arc<FeatureSet>,
    /// Shared feature set for watch mode.
    watch_feature_set: Arc<FeatureSet>,

    /// Internal channel for stealing the compiler thread.
    steal_tx: mpsc::UnboundedSender<Interrupt<Self>>,
    steal_rx: mpsc::UnboundedReceiver<Interrupt<Self>>,

    suspend_state: SuspendState,
}

impl<C: Compiler + ShadowApi + Send + 'static> CompileServerActor<C>
where
    C::World: for<'files> codespan_reporting::files::Files<'files, FileId = TypstFileId>,
{
    pub fn new_with_features(compiler: C, entry: EntryState, feature_set: FeatureSet) -> Self {
        let (steal_tx, steal_rx) = mpsc::unbounded_channel();

        Self {
            compiler: CompileReporter::new(compiler)
                .with_generic_reporter(ConsoleDiagReporter::default()),

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

            steal_tx,
            steal_rx,

            suspend_state: SuspendState {
                suspended: entry.is_inactive(),
                dirty: false,
            },
        }
    }

    /// Create a new compiler thread.
    pub fn new(compiler: C, entry: EntryState) -> Self {
        Self::new_with_features(compiler, entry, FeatureSet::default())
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
            let compiled = self.compiler.compile(&mut env);
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
            self.compiler.compile(&mut env).ok();
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
        let fs_tx = self.steal_tx.clone();
        tokio::spawn(watch_deps(dep_rx, move |event| {
            log_send_error("fs_event", fs_tx.send(Interrupt::Fs(event)));
        }));

        // Spawn compiler thread.
        let thread_builder = std::thread::Builder::new().name("typst-compiler".to_owned());
        let compile_thread = thread_builder.spawn(move || {
            log::debug!("CompileServerActor: initialized");

            // Wait for first events.
            'event_loop: while let Some(mut event) = self.steal_rx.blocking_recv() {
                // Accumulate events, the order of processing which is critical.
                let mut need_compile = false;

                'accumulate: loop {
                    // Warp the logical clock by one.
                    self.logical_tick += 1;

                    if let Interrupt::Settle(e) = event {
                        log::info!("CompileServerActor: requested stop");
                        e.send(()).ok();
                        break 'event_loop;
                    }
                    if matches!(event, Interrupt::Task(_)) && need_compile {
                        self.compile(&compiler_ack);
                        need_compile = false;
                    }
                    need_compile |= self.process(event, &compiler_ack);

                    match self.steal_rx.try_recv() {
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

    pub(crate) fn change_entry(&mut self, entry: EntryState) {
        self.suspend_state.suspended = entry.is_inactive();
        if !self.suspend_state.suspended && self.suspend_state.dirty {
            self.steal_tx.send(Interrupt::Compile).ok();
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

        // Compile the document.
        let mut env = self.make_env(self.watch_feature_set.clone());
        self.latest_doc = self.compiler.compile(&mut env).ok();
        if self.latest_doc.is_some() {
            self.latest_success_doc = self.latest_doc.clone();
        }

        // Evict compilation cache.
        let evict_start = std::time::Instant::now();
        comemo::evict(30);
        let elapsed = evict_start.elapsed();
        log::info!("CompileServerActor: evict compilation cache in {elapsed:?}",);

        // Notify the new file dependencies.
        let mut deps = vec![];
        self.compiler
            .iter_dependencies(&mut |dep, _| deps.push(dep.clone()));
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
                self.compiler.notify_fs_event(event);

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
            self.compiler.reset_shadow();
        }
        match event {
            MemoryEvent::Update(event) | MemoryEvent::Sync(event) => {
                for removes in event.removes {
                    let _ = self.compiler.unmap_shadow(&removes);
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

                    let _ = self.compiler.map_shadow(&p, insert_file);
                }
            }
        }
    }
}

impl<C: Compiler> CompileServerActor<C> {
    pub fn with_watch(mut self, enable_watch: bool) -> Self {
        self.enable_watch = enable_watch;
        self
    }

    pub fn client(&self) -> CompileClient<Self> {
        let intr_tx = self.steal_tx.clone();
        CompileClient { intr_tx }
    }

    pub fn document(&self) -> Option<Arc<TypstDocument>> {
        self.latest_doc.clone()
    }
}

#[derive(Debug, Clone)]
pub struct CompileClient<Ctx> {
    intr_tx: mpsc::UnboundedSender<Interrupt<Ctx>>,
}

impl<Ctx> CompileClient<Ctx> {
    pub fn faked() -> Self {
        let (intr_tx, _) = mpsc::unbounded_channel();
        Self { intr_tx }
    }

    fn steal_inner<Ret: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Ctx) -> Ret + Send + 'static,
    ) -> ZResult<oneshot::Receiver<Ret>> {
        let (tx, rx) = oneshot::channel();

        let task = Box::new(move |this: &mut Ctx| {
            if tx.send(f(this)).is_err() {
                // Receiver was dropped. The main thread may have exited, or the request may
                // have been cancelled.
                log::warn!("could not send back return value from Typst thread");
            }
        });

        self.intr_tx
            .send(Interrupt::Task(task))
            .map_err(map_string_err("failed to send steal request"))?;
        Ok(rx)
    }

    pub fn steal<Ret: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Ctx) -> Ret + Send + 'static,
    ) -> ZResult<Ret> {
        utils::threaded_receive(self.steal_inner(f)?)
    }

    pub fn settle(&self) -> ZResult<()> {
        let (tx, rx) = oneshot::channel();
        // very weird if this is error, we unwrap it.
        self.intr_tx
            .send(Interrupt::Settle(tx))
            .map_err(map_string_err("failed to send settle request"))?;
        utils::threaded_receive(rx)
    }

    /// Steal the compiler thread and run the given function.
    pub async fn steal_async<Ret: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Ctx, tokio::runtime::Handle) -> Ret + Send + 'static,
    ) -> ZResult<Ret> {
        // get current async handle
        let handle = tokio::runtime::Handle::current();
        self.steal_inner(move |this: &mut Ctx| f(this, handle.clone()))?
            .await
            .map_err(map_string_err("failed to call steal_async"))
    }

    pub fn add_memory_changes(&self, event: MemoryEvent) {
        log_send_error("mem_event", self.intr_tx.send(Interrupt::Memory(event)));
    }
}

#[derive(Debug, Serialize)]
pub struct DocToSrcJumpInfo {
    pub filepath: String,
    pub start: Option<(usize, usize)>, // row, column
    pub end: Option<(usize, usize)>,
}

// todo: remove constraint to CompilerWorld
impl<F: CompilerFeat, Ctx: Compiler<World = CompilerWorld<F>>>
    CompileClient<CompileServerActor<Ctx>>
where
    Ctx::World: EntryManager,
{
    /// fixme: character is 0-based, UTF-16 code unit.
    /// We treat it as UTF-8 now.
    pub async fn resolve_src_to_doc_jump(
        &self,
        filepath: PathBuf,
        line: usize,
        character: usize,
    ) -> ZResult<Option<Position>> {
        self.steal_async(move |this, _| {
            let doc = this.document()?;

            let world = this.compiler.world();

            let relative_path = filepath
                .strip_prefix(&this.compiler.world().workspace_root()?)
                .ok()?;

            let source_id = TypstFileId::new(None, VirtualPath::new(relative_path));
            let source = world.source(source_id).ok()?;
            let cursor = source.line_column_to_byte(line, character)?;

            jump_from_cursor(&doc, &source, cursor)
        })
        .await
    }

    /// fixme: character is 0-based, UTF-16 code unit.
    /// We treat it as UTF-8 now.
    pub async fn resolve_src_location(
        &self,
        loc: SourceLocation,
    ) -> ZResult<Option<SourceSpanOffset>> {
        self.steal_async(move |this, _| {
            let world = this.compiler.world();

            let filepath = Path::new(&loc.filepath);
            let relative_path = filepath
                .strip_prefix(&this.compiler.world().workspace_root()?)
                .ok()?;

            let source_id = TypstFileId::new(None, VirtualPath::new(relative_path));
            let source = world.source(source_id).ok()?;
            let cursor = source.line_column_to_byte(loc.pos.line, loc.pos.column)?;

            let node = LinkedNode::new(source.root()).leaf_at(cursor)?;
            if node.kind() != SyntaxKind::Text {
                return None;
            }
            let span = node.span();
            // todo: unicode char
            let offset = cursor.saturating_sub(node.offset());

            Some(SourceSpanOffset { span, offset })
        })
        .await
    }

    pub async fn resolve_span(&self, span: Span) -> ZResult<Option<DocToSrcJumpInfo>> {
        self.resolve_span_and_offset(span, None).await
    }

    pub async fn resolve_span_and_offset(
        &self,
        span: Span,
        offset: Option<usize>,
    ) -> ZResult<Option<DocToSrcJumpInfo>> {
        let resolve_off =
            |src: &Source, off: usize| src.byte_to_line(off).zip(src.byte_to_column(off));

        self.steal_async(move |this, _| {
            let world = this.compiler.world();
            let src_id = span.id()?;
            let source = world.source(src_id).ok()?;
            let mut range = source.find(span)?.range();
            if let Some(off) = offset {
                if off < range.len() {
                    range.start += off;
                }
            }
            let filepath = world.path_for_id(src_id).ok()?;
            Some(DocToSrcJumpInfo {
                filepath: filepath.to_string_lossy().to_string(),
                start: resolve_off(&source, range.start),
                end: resolve_off(&source, range.end),
            })
        })
        .await
    }
}

/// Find the output location in the document for a cursor position.
pub fn jump_from_cursor(
    document: &TypstDocument,
    source: &Source,
    cursor: usize,
) -> Option<Position> {
    let node = LinkedNode::new(source.root()).leaf_at(cursor)?;
    if node.kind() != SyntaxKind::Text {
        return None;
    }

    let mut min_dis = u64::MAX;
    let mut p = Point::default();
    let mut ppage = 0usize;

    let span = node.span();
    for (i, page) in document.pages.iter().enumerate() {
        let t_dis = min_dis;
        if let Some(pos) = find_in_frame(&page.frame, span, &mut min_dis, &mut p) {
            return Some(Position {
                page: NonZeroUsize::new(i + 1)?,
                point: pos,
            });
        }
        if t_dis != min_dis {
            ppage = i;
        }
    }

    if min_dis == u64::MAX {
        return None;
    }

    Some(Position {
        page: NonZeroUsize::new(ppage + 1)?,
        point: p,
    })
}

/// Find the position of a span in a frame.
fn find_in_frame(frame: &Frame, span: Span, min_dis: &mut u64, p: &mut Point) -> Option<Point> {
    for (mut pos, item) in frame.items() {
        if let FrameItem::Group(group) = item {
            // TODO: Handle transformation.
            if let Some(point) = find_in_frame(&group.frame, span, min_dis, p) {
                return Some(point + pos);
            }
        }

        if let FrameItem::Text(text) = item {
            for glyph in &text.glyphs {
                if glyph.span.0 == span {
                    return Some(pos);
                }
                if glyph.span.0.id() == span.id() {
                    let dis = glyph.span.0.number().abs_diff(span.number());
                    if dis < *min_dis {
                        *min_dis = dis;
                        *p = pos;
                    }
                }
                pos.x += glyph.x_advance.at(text.size);
            }
        }
    }

    None
}

#[inline]
fn log_send_error<T>(chan: &'static str, res: Result<(), mpsc::error::SendError<T>>) -> bool {
    res.map_err(|err| log::warn!("CompileServerActor: send to {chan} error: {err}"))
        .is_ok()
}
