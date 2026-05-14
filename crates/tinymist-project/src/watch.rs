//! upstream <https://github.com/rust-lang/rust-analyzer/tree/master/crates/vfs-notify>
//!
//! An implementation of `watch_deps` using `notify` crate.
//!
//! The file watching bits here are untested and quite probably buggy. For this
//! reason, by default we don't watch files and rely on editor's file watching
//! capabilities.
//!
//! Hopefully, one day a reliable file watching/walking crate appears on
//! crates.io, and we can reduce this to trivial glue code.

use std::{collections::HashMap, fmt, path::Path};

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use tinymist_std::{ImmutPath, error::IgnoreLogging};
use tinymist_world::vfs::notify::NotifyDeps;
use tokio::sync::mpsc;
use typst::diag::{FileError, FileResult};

use tinymist_world::vfs::{
    Bytes, FileChangeSet, FileSnapshot, PathAccessModel,
    notify::{FilesystemEvent, NotifyMessage, UpstreamUpdateEvent},
    system::SystemAccessModel,
};

type WatcherPair = (RecommendedWatcher, mpsc::UnboundedReceiver<NotifyEvent>);
type NotifyEvent = notify::Result<notify::Event>;
type FileEntry = (/* key */ ImmutPath, /* value */ FileSnapshot);

trait NotifyActorAccess: fmt::Debug + Send + Sync {
    fn content(&self, src: &Path) -> FileResult<Bytes>;

    fn is_watchable_file(&self, src: &Path) -> bool;
}

#[derive(Debug)]
struct SystemNotifyActorAccess(SystemAccessModel);

impl NotifyActorAccess for SystemNotifyActorAccess {
    fn content(&self, src: &Path) -> FileResult<Bytes> {
        self.0.content(src)
    }

    fn is_watchable_file(&self, src: &Path) -> bool {
        src.metadata().is_ok_and(|meta| !meta.is_dir())
    }
}

#[derive(Debug)]
enum NotifyWatcher {
    System(WatcherPair),
    #[cfg(test)]
    Fake(FakeWatcher),
}

impl NotifyWatcher {
    async fn recv(&mut self) -> Option<NotifyEvent> {
        match self {
            Self::System((_, watcher_receiver)) => watcher_receiver.recv().await,
            #[cfg(test)]
            Self::Fake(_) => None,
        }
    }

    fn watch(&mut self, path: &Path, recursive_mode: RecursiveMode) -> notify::Result<()> {
        match self {
            Self::System((watcher, _)) => watcher.watch(path, recursive_mode),
            #[cfg(test)]
            Self::Fake(watcher) => watcher.watch(path),
        }
    }

    fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
        match self {
            Self::System((watcher, _)) => watcher.unwatch(path),
            #[cfg(test)]
            Self::Fake(watcher) => watcher.unwatch(path),
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
enum FakeWatchCommand {
    Watch(std::path::PathBuf),
    Unwatch(std::path::PathBuf),
}

#[cfg(test)]
#[derive(Debug, Default, Clone)]
struct FakeWatchCommands(std::sync::Arc<std::sync::Mutex<Vec<FakeWatchCommand>>>);

#[cfg(test)]
impl FakeWatchCommands {
    fn push(&self, command: FakeWatchCommand) {
        self.0
            .lock()
            .expect("fake watch commands poisoned")
            .push(command);
    }

    fn take(&self) -> Vec<FakeWatchCommand> {
        std::mem::take(&mut *self.0.lock().expect("fake watch commands poisoned"))
    }
}

#[cfg(test)]
#[derive(Debug)]
struct FakeWatcher {
    commands: FakeWatchCommands,
}

#[cfg(test)]
impl FakeWatcher {
    fn watch(&self, path: &Path) -> notify::Result<()> {
        self.commands
            .push(FakeWatchCommand::Watch(path.to_path_buf()));
        Ok(())
    }

    fn unwatch(&self, path: &Path) -> notify::Result<()> {
        self.commands
            .push(FakeWatchCommand::Unwatch(path.to_path_buf()));
        Ok(())
    }
}

/// The state of a watched file.
///
/// It is used to determine some dirty editors' implementation.
#[derive(Debug)]
enum WatchState {
    /// The file is stable, which means we believe that it keeps synchronized
    /// as expected.
    Stable,
    /// The file is empty or removed, but there is a chance that the file is not
    /// stable. So we need to recheck the file after a while.
    EmptyOrRemoval {
        recheck_at: usize,
        payload: FileSnapshot,
    },
}

/// By default, the state is stable.
impl Default for WatchState {
    fn default() -> Self {
        Self::Stable
    }
}

/// The data entry of a watched file.
#[derive(Debug)]
struct WatchedEntry {
    /// The lifetime of the entry.
    ///
    /// The entry will be removed if the entry is too old.
    // todo: generalize lifetime
    lifetime: usize,
    /// A flag for whether it is really watching.
    watching: bool,
    /// A flag for watch update.
    seen: bool,
    /// The state of the entry.
    state: WatchState,
    /// Previous content of the file.
    prev: Option<FileSnapshot>,
}

/// Self produced event that check whether the file is stable after a while.
#[derive(Debug)]
struct UndeterminedNotifyEvent {
    /// The time when the event is produced.
    at_realtime: tinymist_std::time::Instant,
    /// The logical tick when the event is produced.
    at_logical_tick: usize,
    /// The path of the file.
    path: ImmutPath,
}

// Drop order is significant.
/// The actor that watches files.
/// It is used to watch files and send events to the consumers
#[derive(Debug)]
pub struct NotifyActor<F: FnMut(FilesystemEvent)> {
    /// The access model of the actor.
    inner: Box<dyn NotifyActorAccess>,

    /// The lifetime of the watched files.
    lifetime: usize,
    /// The logical tick of the actor.
    logical_tick: usize,

    /// Internal channel for recheck events.
    undetermined_send: mpsc::UnboundedSender<UndeterminedNotifyEvent>,
    undetermined_recv: mpsc::UnboundedReceiver<UndeterminedNotifyEvent>,

    /// The hold entries for watching, one entry for per file.
    watched_entries: HashMap<ImmutPath, WatchedEntry>,

    interrupted_by_events: F,

    /// The builtin watcher object.
    watcher: Option<NotifyWatcher>,
}

impl<F: FnMut(FilesystemEvent) + Send + Sync> NotifyActor<F> {
    /// Create a new actor.
    pub fn new(interrupted_by_events: F) -> Self {
        let (undetermined_send, undetermined_recv) = mpsc::unbounded_channel();
        let (watcher_tx, watcher_rx) = mpsc::unbounded_channel();
        let watcher = log_notify_error(
            RecommendedWatcher::new(
                move |event| {
                    watcher_tx.send(event).log_error("failed to send fs notify");
                },
                Config::default(),
            ),
            "failed to create watcher",
        );

        NotifyActor {
            inner: Box::new(SystemNotifyActorAccess(SystemAccessModel)),
            // we start from 1 to distinguish from 0 (default value)
            lifetime: 1,
            logical_tick: 1,

            interrupted_by_events,

            undetermined_send,
            undetermined_recv,

            watched_entries: HashMap::new(),
            watcher: watcher.map(|it| NotifyWatcher::System((it, watcher_rx))),
        }
    }

    #[cfg(test)]
    fn new_for_test(
        inner: Box<dyn NotifyActorAccess>,
        commands: FakeWatchCommands,
        interrupted_by_events: F,
    ) -> Self {
        let (undetermined_send, undetermined_recv) = mpsc::unbounded_channel();

        NotifyActor {
            inner,
            // we start from 1 to distinguish from 0 (default value)
            lifetime: 1,
            logical_tick: 1,

            interrupted_by_events,

            undetermined_send,
            undetermined_recv,

            watched_entries: HashMap::new(),
            watcher: Some(NotifyWatcher::Fake(FakeWatcher { commands })),
        }
    }

    /// Get the notify event from the watcher.
    async fn get_notify_event(watcher: &mut Option<NotifyWatcher>) -> Option<NotifyEvent> {
        match watcher {
            Some(watcher) => watcher.recv().await,
            None => None,
        }
    }

    /// Main loop of the actor.
    pub async fn run(mut self, mut inbox: mpsc::UnboundedReceiver<NotifyMessage>) {
        use NotifyMessage::*;
        /// The event of the actor.
        #[derive(Debug)]
        enum ActorEvent {
            /// Recheck the notify event.
            ReCheck(UndeterminedNotifyEvent),
            /// external message to change notifier's state
            Message(Option<NotifyMessage>),
            /// notify event from builtin watcher
            NotifyEvent(NotifyEvent),
        }

        'event_loop: loop {
            // Get the event from the inbox or the watcher.
            let event = tokio::select! {
                it = inbox.recv() => ActorEvent::Message(it),
                Some(it) = Self::get_notify_event(&mut self.watcher) => ActorEvent::NotifyEvent(it),
                Some(it) = self.undetermined_recv.recv() => ActorEvent::ReCheck(it),
            };

            // Increase the logical tick per event.
            self.logical_tick += 1;

            // log::info!("vfs-notify event {event:?}");
            // function entries to handle some event
            match event {
                ActorEvent::Message(None) => {
                    log::info!("NotifyActor: failed to get event, exiting...");
                    break 'event_loop;
                }
                ActorEvent::Message(Some(Settle)) => {
                    log::info!("NotifyActor: settle event received");
                    break 'event_loop;
                }
                ActorEvent::Message(Some(UpstreamUpdate(event))) => {
                    self.invalidate_upstream(event);
                }
                ActorEvent::Message(Some(SyncDependency(paths))) => {
                    if let Some(changeset) = self.update_watches(paths.as_ref()) {
                        (self.interrupted_by_events)(FilesystemEvent::Update(changeset, true));
                    }
                }
                ActorEvent::NotifyEvent(event) => {
                    // log::info!("notify event {event:?}");
                    if let Some(event) = log_notify_error(event, "failed to notify") {
                        self.notify_event(event);
                    }
                }
                ActorEvent::ReCheck(event) => {
                    self.recheck_notify_event(event).await;
                }
            }
        }

        log::info!("NotifyActor: exited");
    }

    /// Update the watches of corresponding invalidation
    fn invalidate_upstream(&mut self, event: UpstreamUpdateEvent) {
        // Update watches of invalidated files.
        let changeset = self.update_watches(&event.invalidates).unwrap_or_default();

        // Send the event to the consumer.
        (self.interrupted_by_events)(FilesystemEvent::UpstreamUpdate {
            changeset,
            upstream_event: Some(event),
        });
    }

    /// Update the watches of corresponding files.
    fn update_watches(&mut self, paths: &dyn NotifyDeps) -> Option<FileChangeSet> {
        // Increase the lifetime per external message.
        self.lifetime += 1;

        let mut changeset = FileChangeSet::default();

        // Mark the old entries as unseen.
        for path in self.watched_entries.values_mut() {
            path.seen = false;
        }

        // Update watched entries.
        //
        // Also check whether the file is updated since there is a window
        // between unwatch the file and watch the file again.
        paths.dependencies(&mut |path| {
            let mut contained = false;
            // Update or insert the entry with the new lifetime.
            let entry = self
                .watched_entries
                .entry(path.clone())
                .and_modify(|watch_entry| {
                    contained = true;
                    watch_entry.lifetime = self.lifetime;
                })
                .or_insert_with(|| WatchedEntry {
                    lifetime: self.lifetime,
                    watching: false,
                    seen: false,
                    state: WatchState::Stable,
                    prev: None,
                });

            if entry.seen {
                return;
            }
            entry.seen = true;

            if self.watcher.is_some() {
                let watchable = self.inner.is_watchable_file(path.as_ref());

                // Case1. meta = Err(..) We cannot get the metadata successfully, so we
                // are okay to ignore this file for watching.
                //
                // Case2. meta = Ok(..) Watch the file if it's not watched.
                if watchable && (!contained || !entry.watching) {
                    log::debug!("watching {path:?}");
                    if let Some(watcher) = &mut self.watcher {
                        entry.watching = log_notify_error(
                            watcher.watch(path.as_ref(), RecursiveMode::NonRecursive),
                            "failed to watch",
                        )
                        .is_some();
                    }
                }

                changeset.may_insert(self.notify_entry_update(path.clone()));
            } else {
                let watched = self.inner.content(path);
                changeset.inserts.push((path.clone(), watched.into()));
            }
        });

        // Remove old entries.
        // Note: since we have increased the lifetime, it is safe to remove the
        // old entries after updating the watched entries.
        self.watched_entries.retain(|path, entry| {
            if !entry.seen && entry.watching {
                log::debug!("unwatch {path:?}");
                if let Some(watcher) = &mut self.watcher {
                    log_notify_error(watcher.unwatch(path), "failed to unwatch");
                    entry.watching = false;
                }
            }

            let fresh = self.lifetime - entry.lifetime < 30;
            if !fresh {
                changeset.removes.push(path.clone());
            }
            fresh
        });

        (!changeset.is_empty()).then_some(changeset)
    }

    /// Notify the event from the builtin watcher.
    fn notify_event(&mut self, event: notify::Event) {
        // Account file updates.
        let mut changeset = FileChangeSet::default();
        for path in event.paths.iter() {
            // todo: remove this clone: path.into()
            changeset.may_insert(self.notify_entry_update(path.as_path().into()));
        }

        // Workaround for notify-rs' implicit unwatch on remove/rename
        // (triggered by some editors when saving files) with the
        // inotify backend. By keeping track of the potentially
        // unwatched files, we can allow those we still depend on to be
        // watched again later on.
        if matches!(
            event.kind,
            notify::EventKind::Remove(notify::event::RemoveKind::File)
                | notify::EventKind::Modify(notify::event::ModifyKind::Name(
                    notify::event::RenameMode::From
                ))
        ) {
            for path in &event.paths {
                let Some(entry) = self.watched_entries.get_mut(path.as_path()) else {
                    continue;
                };
                if !entry.watching {
                    continue;
                }
                // Remove affected path from the watched map to restart
                // watching on it later again.
                if let Some(watcher) = &mut self.watcher {
                    log_notify_error(watcher.unwatch(path), "failed to unwatch");
                }
                entry.watching = false;
            }
        }

        // Send file updates.
        if !changeset.is_empty() {
            (self.interrupted_by_events)(FilesystemEvent::Update(changeset, false));
        }
    }

    /// Notify any update of the file entry
    fn notify_entry_update(&mut self, path: ImmutPath) -> Option<FileEntry> {
        // The following code in rust-analyzer is commented out
        // todo: check whether we need this
        // if meta.file_type().is_dir() && self
        //   .watched_entries.iter().any(|entry| entry.contains_dir(&path))
        // {
        //     self.watch(path);
        //     return None;
        // }

        // Find entry and continue
        let entry = self.watched_entries.get_mut(&path)?;

        // Check meta, path, and content
        let file = FileSnapshot::from(self.inner.content(&path));

        // Check state in fast path: compare state, return None on not sending
        // the file change
        match (entry.prev.as_deref(), file.as_ref()) {
            // update the content of the entry in the following cases:
            // + Case 1: previous content is clear
            // + Case 2: previous content is not clear but some error, and the
            // current content is ok
            (None, ..) | (Some(Err(..)), Ok(..)) => {}
            // Meet some error currently
            (Some(it), Err(err)) => match &mut entry.state {
                // If the file is stable, check whether the editor is removing
                // or truncating the file. They are possibly flushing the file
                // but not finished yet.
                WatchState::Stable => {
                    if matches!(err.as_ref(), FileError::NotFound(..) | FileError::Other(..)) {
                        entry.state = WatchState::EmptyOrRemoval {
                            recheck_at: self.logical_tick,
                            payload: file.clone(),
                        };
                        entry.prev = Some(file);
                        let event = UndeterminedNotifyEvent {
                            at_realtime: tinymist_std::time::Instant::now(),
                            at_logical_tick: self.logical_tick,
                            path: path.clone(),
                        };
                        log_send_error("recheck", self.undetermined_send.send(event));
                        return None;
                    }
                    // Otherwise, we push the error to the consumer.

                    // Ignores the error if the error is stable
                    if it.as_ref().is_err_and(|it| it == err) {
                        return None;
                    }
                }

                // Very complicated case of check error sequence, so we simplify
                // a bit, we regard any subsequent error as the same error.
                WatchState::EmptyOrRemoval { payload, .. } => {
                    // update payload
                    *payload = file;
                    return None;
                }
            },
            // Compare content for transitional the state
            (Some(Ok(prev_content)), Ok(next_content)) => {
                // So far it is accurately no change for the file, skip it
                if prev_content == next_content {
                    return None;
                }

                match entry.state {
                    // If the file is stable, check whether the editor is
                    // removing or truncating the file. They are possibly
                    // flushing the file but not finished yet.
                    WatchState::Stable => {
                        if next_content.is_empty() {
                            entry.state = WatchState::EmptyOrRemoval {
                                recheck_at: self.logical_tick,
                                payload: file.clone(),
                            };
                            entry.prev = Some(file);
                            let event = UndeterminedNotifyEvent {
                                at_realtime: tinymist_std::time::Instant::now(),
                                at_logical_tick: self.logical_tick,
                                path,
                            };
                            log_send_error("recheck", self.undetermined_send.send(event));
                            return None;
                        }
                    }

                    // Still empty
                    WatchState::EmptyOrRemoval { .. } if next_content.is_empty() => return None,
                    // Otherwise, we push the diff to the consumer.
                    WatchState::EmptyOrRemoval { .. } => {}
                }
            }
        };

        // Send the update to the consumer
        // Update the entry according to the state
        entry.state = WatchState::Stable;
        entry.prev = Some(file.clone());

        // Slow path: trigger the file change for consumer
        Some((path, file))
    }

    /// Recheck the notify event after a while.
    async fn recheck_notify_event(&mut self, event: UndeterminedNotifyEvent) -> Option<()> {
        let now = tinymist_std::time::Instant::now();
        log::debug!("recheck event {event:?} at {now:?}");

        // The async scheduler is not accurate, so we need to ensure a window here
        let reserved = now - event.at_realtime;
        if reserved < tinymist_std::time::Duration::from_millis(50) {
            let send = self.undetermined_send.clone();
            tokio::spawn(async move {
                // todo: sleep in browser
                tokio::time::sleep(tinymist_std::time::Duration::from_millis(50) - reserved).await;
                log_send_error("reschedule", send.send(event));
            });
            return None;
        }

        // Check whether the entry is still valid
        let entry = self.watched_entries.get_mut(&event.path)?;

        // Check the state of the entry
        match std::mem::take(&mut entry.state) {
            // If the entry is stable, we do nothing
            WatchState::Stable => {}
            // If the entry is not stable, and no other event is produced after
            // this event, we send the event to the consumer.
            WatchState::EmptyOrRemoval {
                recheck_at,
                payload,
            } => {
                if recheck_at == event.at_logical_tick {
                    log::debug!("notify event real happened {event:?}, state: {payload:?}");

                    // Send the underlying change to the consumer
                    let mut changeset = FileChangeSet::default();
                    changeset.inserts.push((event.path, payload));

                    (self.interrupted_by_events)(FilesystemEvent::Update(changeset, false));
                }
            }
        };

        Some(())
    }
}

#[inline]
fn log_notify_error<T>(res: notify::Result<T>, reason: &'static str) -> Option<T> {
    res.map_err(|err| log::warn!("{reason}: notify error: {err}"))
        .ok()
}

#[inline]
fn log_send_error<T>(chan: &'static str, res: Result<(), mpsc::error::SendError<T>>) -> bool {
    res.map_err(|err| log::warn!("NotifyActor: send to {chan} error: {err}"))
        .is_ok()
}

/// Watches on a set of *files*.
pub async fn watch_deps(
    inbox: mpsc::UnboundedReceiver<NotifyMessage>,
    interrupted_by_events: impl FnMut(FilesystemEvent) + Send + Sync + 'static,
) {
    log::info!("NotifyActor: start watching files...");
    // Watch messages to notify
    spawn_watch_deps(inbox, interrupted_by_events);
}

fn spawn_watch_deps(
    inbox: mpsc::UnboundedReceiver<NotifyMessage>,
    interrupted_by_events: impl FnMut(FilesystemEvent) + Send + Sync + 'static,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(NotifyActor::new(interrupted_by_events).run(inbox))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        path::{Path, PathBuf},
        sync::{Arc, Mutex},
    };

    use notify::event::{CreateKind, DataChange, ModifyKind, RemoveKind, RenameMode};

    use super::*;

    type EventSink = Box<dyn FnMut(FilesystemEvent) + Send + Sync>;

    // Matrix coverage note:
    // Different notify backends can report the same editor operation as
    // rename-from/rename-to pairs, RenameMode::Both, create/modify batches, or
    // multi-path modify events. These deterministic rows model the equivalent
    // actor inputs directly so the core expectations do not depend on a host
    // backend's timing or coalescing policy.

    #[derive(Debug, Clone)]
    struct TestFile {
        snapshot: FileSnapshot,
        watchable: bool,
    }

    #[derive(Debug, Default, Clone)]
    struct TestAccess {
        files: Arc<Mutex<HashMap<PathBuf, TestFile>>>,
    }

    impl TestAccess {
        fn set_content(&self, path: &ImmutPath, content: &str) {
            self.set_snapshot(path, content_snapshot(content), true);
        }

        fn set_empty(&self, path: &ImmutPath) {
            self.set_content(path, "");
        }

        fn set_error(&self, path: &ImmutPath) {
            self.set_snapshot(
                path,
                Err::<Bytes, FileError>(FileError::Other(None)).into(),
                true,
            );
        }

        fn set_missing(&self, path: &ImmutPath) {
            self.files
                .lock()
                .expect("test access poisoned")
                .remove(path.as_ref());
        }

        fn set_snapshot(&self, path: &ImmutPath, snapshot: FileSnapshot, watchable: bool) {
            self.files.lock().expect("test access poisoned").insert(
                path.as_ref().to_path_buf(),
                TestFile {
                    snapshot,
                    watchable,
                },
            );
        }
    }

    impl NotifyActorAccess for TestAccess {
        fn content(&self, src: &Path) -> FileResult<Bytes> {
            self.files
                .lock()
                .expect("test access poisoned")
                .get(src)
                .map_or_else(
                    || Err(FileError::NotFound(src.into())),
                    |file| file.snapshot.content().cloned(),
                )
        }

        fn is_watchable_file(&self, src: &Path) -> bool {
            self.files
                .lock()
                .expect("test access poisoned")
                .get(src)
                .is_some_and(|file| file.watchable)
        }
    }

    #[derive(Debug)]
    enum MatrixInput {
        SyncDependency(Vec<ImmutPath>),
        UpstreamInvalidation {
            invalidates: Vec<ImmutPath>,
            opaque: usize,
        },
        WatcherEvent {
            kind: notify::EventKind,
            paths: Vec<ImmutPath>,
        },
        DelayedRecheck(ImmutPath),
        DelayedRecheckAt {
            path: ImmutPath,
            recheck_at: usize,
        },
    }

    struct NotifyActorHarness {
        access: TestAccess,
        commands: FakeWatchCommands,
        events: Arc<Mutex<Vec<FilesystemEvent>>>,
        actor: NotifyActor<EventSink>,
    }

    impl NotifyActorHarness {
        fn new() -> Self {
            let access = TestAccess::default();
            let commands = FakeWatchCommands::default();
            let events = Arc::new(Mutex::new(Vec::new()));
            let sink_events = events.clone();
            let sink: EventSink = Box::new(move |event| {
                sink_events
                    .lock()
                    .expect("test event sink poisoned")
                    .push(event);
            });
            let actor = NotifyActor::new_for_test(Box::new(access.clone()), commands.clone(), sink);

            Self {
                access,
                commands,
                events,
                actor,
            }
        }

        async fn apply(&mut self, input: MatrixInput) {
            self.actor.logical_tick += 1;

            match input {
                MatrixInput::SyncDependency(paths) => {
                    if let Some(changeset) = self.actor.update_watches(&paths) {
                        (self.actor.interrupted_by_events)(FilesystemEvent::Update(
                            changeset, true,
                        ));
                    }
                }
                MatrixInput::UpstreamInvalidation {
                    invalidates,
                    opaque,
                } => {
                    self.actor.invalidate_upstream(UpstreamUpdateEvent {
                        invalidates,
                        opaque: Box::new(opaque),
                    });
                }
                MatrixInput::WatcherEvent { kind, paths } => {
                    self.actor.notify_event(notify_event(kind, paths));
                }
                MatrixInput::DelayedRecheck(path) => {
                    let recheck_at = self.pending_recheck_at(&path);
                    self.force_recheck_at(path, recheck_at).await;
                }
                MatrixInput::DelayedRecheckAt { path, recheck_at } => {
                    self.force_recheck_at(path, recheck_at).await;
                }
            }
        }

        fn pending_recheck_at(&self, path: &ImmutPath) -> usize {
            match self
                .actor
                .watched_entries
                .get(path)
                .expect("watched entry must exist for delayed recheck")
                .state
            {
                WatchState::EmptyOrRemoval { recheck_at, .. } => recheck_at,
                WatchState::Stable => panic!("watched entry must be pending recheck"),
            }
        }

        async fn force_recheck_at(&mut self, path: ImmutPath, recheck_at: usize) {
            self.actor
                .recheck_notify_event(UndeterminedNotifyEvent {
                    at_realtime: tinymist_std::time::Instant::now()
                        - tinymist_std::time::Duration::from_millis(60),
                    at_logical_tick: recheck_at,
                    path,
                })
                .await;
        }

        fn take_events(&self) -> Vec<FilesystemEvent> {
            std::mem::take(&mut *self.events.lock().expect("test event sink poisoned"))
        }

        fn assert_no_events(&self) {
            assert!(
                self.events
                    .lock()
                    .expect("test event sink poisoned")
                    .is_empty(),
                "expected no filesystem events"
            );
        }

        fn take_commands(&self) -> Vec<FakeWatchCommand> {
            self.commands.take()
        }

        fn assert_watching(&self, path: &ImmutPath, expected: bool) {
            assert_eq!(
                self.actor
                    .watched_entries
                    .get(path)
                    .map(|entry| entry.watching),
                Some(expected)
            );
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sync_dependency_updates_watch_set_and_changed_contents() {
        let mut harness = NotifyActorHarness::new();
        let first = test_path("sync-first.typ");
        let second = test_path("sync-second.typ");

        harness.access.set_content(&first, "first-v1");
        harness.access.set_content(&second, "second-v1");
        harness
            .apply(MatrixInput::SyncDependency(vec![
                first.clone(),
                second.clone(),
            ]))
            .await;

        assert_eq!(harness.take_commands(), vec![watch(&first), watch(&second)]);
        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_update(
            &events[0],
            true,
            &[
                (&first, ExpectedSnapshot::Content("first-v1")),
                (&second, ExpectedSnapshot::Content("second-v1")),
            ],
        );

        harness.access.set_content(&first, "first-v2");
        harness
            .apply(MatrixInput::SyncDependency(vec![
                first.clone(),
                second.clone(),
            ]))
            .await;

        assert_eq!(harness.take_commands(), Vec::new());
        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_update(
            &events[0],
            true,
            &[(&first, ExpectedSnapshot::Content("first-v2"))],
        );

        harness
            .apply(MatrixInput::SyncDependency(vec![first.clone()]))
            .await;

        assert_eq!(harness.take_commands(), vec![unwatch(&second)]);
        harness.assert_no_events();

        harness.access.set_content(&second, "second-v2");
        harness
            .apply(MatrixInput::SyncDependency(vec![
                first.clone(),
                second.clone(),
            ]))
            .await;

        assert_eq!(harness.take_commands(), vec![watch(&second)]);
        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_update(
            &events[0],
            true,
            &[(&second, ExpectedSnapshot::Content("second-v2"))],
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn create_and_modify_events_update_watched_dependencies() {
        let mut harness = NotifyActorHarness::new();
        let dep = test_path("create-modify.typ");

        harness.access.set_content(&dep, "initial");
        harness
            .apply(MatrixInput::SyncDependency(vec![dep.clone()]))
            .await;
        harness.take_events();
        harness.take_commands();

        harness.access.set_content(&dep, "created");
        harness
            .apply(MatrixInput::WatcherEvent {
                kind: notify::EventKind::Create(CreateKind::File),
                paths: vec![dep.clone()],
            })
            .await;

        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_update(
            &events[0],
            false,
            &[(&dep, ExpectedSnapshot::Content("created"))],
        );

        harness.access.set_content(&dep, "modified");
        harness
            .apply(MatrixInput::WatcherEvent {
                kind: modify_data_event(),
                paths: vec![dep.clone()],
            })
            .await;

        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_update(
            &events[0],
            false,
            &[(&dep, ExpectedSnapshot::Content("modified"))],
        );

        harness
            .apply(MatrixInput::WatcherEvent {
                kind: modify_data_event(),
                paths: vec![dep.clone()],
            })
            .await;

        harness.assert_no_events();
        assert_eq!(harness.take_commands(), Vec::new());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn raw_events_for_unwatched_paths_are_ignored() {
        let mut harness = NotifyActorHarness::new();
        let watched = test_path("watched.typ");
        let unwatched = test_path("unwatched.typ");

        harness.access.set_content(&watched, "watched");
        harness.access.set_content(&unwatched, "unwatched");
        harness
            .apply(MatrixInput::SyncDependency(vec![watched.clone()]))
            .await;
        harness.take_events();
        harness.take_commands();

        harness.access.set_content(&unwatched, "unwatched-change");
        harness
            .apply(MatrixInput::WatcherEvent {
                kind: modify_data_event(),
                paths: vec![unwatched],
            })
            .await;

        harness.assert_no_events();
        assert_eq!(harness.take_commands(), Vec::new());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn remove_and_rename_from_reset_watch_state_and_confirm_changes() {
        let mut harness = NotifyActorHarness::new();
        let dep = test_path("remove-rename-from.typ");

        harness.access.set_content(&dep, "alive");
        harness
            .apply(MatrixInput::SyncDependency(vec![dep.clone()]))
            .await;
        harness.take_events();
        harness.take_commands();

        harness.access.set_missing(&dep);
        harness
            .apply(MatrixInput::WatcherEvent {
                kind: notify::EventKind::Remove(RemoveKind::File),
                paths: vec![dep.clone()],
            })
            .await;

        assert_eq!(harness.take_commands(), vec![unwatch(&dep)]);
        harness.assert_watching(&dep, false);
        harness.assert_no_events();

        harness
            .apply(MatrixInput::DelayedRecheck(dep.clone()))
            .await;

        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_update(&events[0], false, &[(&dep, ExpectedSnapshot::NotFound)]);

        harness.access.set_content(&dep, "restored");
        harness
            .apply(MatrixInput::SyncDependency(vec![dep.clone()]))
            .await;

        assert_eq!(harness.take_commands(), vec![watch(&dep)]);
        harness.assert_watching(&dep, true);
        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_update(
            &events[0],
            true,
            &[(&dep, ExpectedSnapshot::Content("restored"))],
        );

        harness.access.set_missing(&dep);
        harness
            .apply(MatrixInput::WatcherEvent {
                kind: notify::EventKind::Modify(ModifyKind::Name(RenameMode::From)),
                paths: vec![dep.clone()],
            })
            .await;

        assert_eq!(harness.take_commands(), vec![unwatch(&dep)]);
        harness.assert_watching(&dep, false);
        harness.assert_no_events();

        harness
            .apply(MatrixInput::DelayedRecheck(dep.clone()))
            .await;

        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_update(&events[0], false, &[(&dep, ExpectedSnapshot::NotFound)]);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rename_to_paired_rename_and_multi_path_events_are_mapped() {
        let mut harness = NotifyActorHarness::new();
        let rename_to = test_path("rename-to.typ");
        let rename_from = test_path("paired-from.typ");
        let paired_to = test_path("paired-to.typ");
        let multi_first = test_path("multi-first.typ");
        let multi_second = test_path("multi-second.typ");
        let ignored = test_path("multi-ignored.typ");

        for path in [
            &rename_to,
            &rename_from,
            &paired_to,
            &multi_first,
            &multi_second,
        ] {
            harness.access.set_content(path, "initial");
        }
        harness.access.set_content(&ignored, "ignored");
        harness
            .apply(MatrixInput::SyncDependency(vec![
                rename_to.clone(),
                rename_from.clone(),
                paired_to.clone(),
                multi_first.clone(),
                multi_second.clone(),
            ]))
            .await;
        harness.take_events();
        harness.take_commands();

        harness.access.set_content(&rename_to, "rename-to-content");
        harness
            .apply(MatrixInput::WatcherEvent {
                kind: notify::EventKind::Modify(ModifyKind::Name(RenameMode::To)),
                paths: vec![rename_to.clone()],
            })
            .await;

        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_update(
            &events[0],
            false,
            &[(&rename_to, ExpectedSnapshot::Content("rename-to-content"))],
        );
        assert_eq!(harness.take_commands(), Vec::new());

        harness.access.set_missing(&rename_from);
        harness.access.set_content(&paired_to, "paired-to-content");
        harness
            .apply(MatrixInput::WatcherEvent {
                kind: notify::EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                paths: vec![rename_from.clone(), paired_to.clone()],
            })
            .await;

        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_update(
            &events[0],
            false,
            &[(&paired_to, ExpectedSnapshot::Content("paired-to-content"))],
        );
        assert_eq!(harness.take_commands(), Vec::new());

        harness
            .apply(MatrixInput::DelayedRecheck(rename_from.clone()))
            .await;

        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_update(
            &events[0],
            false,
            &[(&rename_from, ExpectedSnapshot::NotFound)],
        );

        harness
            .access
            .set_content(&multi_first, "multi-first-content");
        harness
            .access
            .set_content(&multi_second, "multi-second-content");
        harness.access.set_content(&ignored, "ignored-content");
        harness
            .apply(MatrixInput::WatcherEvent {
                kind: modify_data_event(),
                paths: vec![multi_first.clone(), multi_second.clone(), ignored],
            })
            .await;

        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_update(
            &events[0],
            false,
            &[
                (
                    &multi_first,
                    ExpectedSnapshot::Content("multi-first-content"),
                ),
                (
                    &multi_second,
                    ExpectedSnapshot::Content("multi-second-content"),
                ),
            ],
        );
        assert_eq!(harness.take_commands(), Vec::new());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn unstable_reads_delay_confirmation_and_recover_before_recheck() {
        let mut harness = NotifyActorHarness::new();
        let empty = test_path("unstable-empty.typ");
        let missing = test_path("unstable-missing.typ");
        let errored = test_path("unstable-error.typ");
        let recovery = test_path("unstable-recovery.typ");

        for path in [&empty, &missing, &errored, &recovery] {
            harness.access.set_content(path, "stable");
        }
        harness
            .apply(MatrixInput::SyncDependency(vec![
                empty.clone(),
                missing.clone(),
                errored.clone(),
                recovery.clone(),
            ]))
            .await;
        harness.take_events();
        harness.take_commands();

        harness.access.set_empty(&empty);
        harness
            .apply(MatrixInput::WatcherEvent {
                kind: modify_data_event(),
                paths: vec![empty.clone()],
            })
            .await;
        harness.assert_no_events();
        harness
            .apply(MatrixInput::DelayedRecheck(empty.clone()))
            .await;

        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_update(
            &events[0],
            false,
            &[(&empty, ExpectedSnapshot::Content(""))],
        );

        harness.access.set_missing(&missing);
        harness
            .apply(MatrixInput::WatcherEvent {
                kind: modify_data_event(),
                paths: vec![missing.clone()],
            })
            .await;
        harness.assert_no_events();
        harness
            .apply(MatrixInput::DelayedRecheck(missing.clone()))
            .await;

        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_update(&events[0], false, &[(&missing, ExpectedSnapshot::NotFound)]);

        harness.access.set_error(&errored);
        harness
            .apply(MatrixInput::WatcherEvent {
                kind: modify_data_event(),
                paths: vec![errored.clone()],
            })
            .await;
        harness.assert_no_events();
        harness
            .apply(MatrixInput::DelayedRecheck(errored.clone()))
            .await;

        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_update(&events[0], false, &[(&errored, ExpectedSnapshot::Other)]);

        harness.access.set_empty(&recovery);
        harness
            .apply(MatrixInput::WatcherEvent {
                kind: modify_data_event(),
                paths: vec![recovery.clone()],
            })
            .await;
        harness.assert_no_events();
        let recovery_recheck_at = harness.pending_recheck_at(&recovery);

        harness.access.set_content(&recovery, "recovered");
        harness
            .apply(MatrixInput::WatcherEvent {
                kind: modify_data_event(),
                paths: vec![recovery.clone()],
            })
            .await;

        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_update(
            &events[0],
            false,
            &[(&recovery, ExpectedSnapshot::Content("recovered"))],
        );

        harness
            .apply(MatrixInput::DelayedRecheckAt {
                path: recovery.clone(),
                recheck_at: recovery_recheck_at,
            })
            .await;
        harness.assert_no_events();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn upstream_invalidation_refreshes_watches_and_carries_payload() {
        let mut harness = NotifyActorHarness::new();
        let existing = test_path("upstream-existing.typ");
        let added = test_path("upstream-added.typ");

        harness.access.set_content(&existing, "existing-v1");
        harness
            .apply(MatrixInput::SyncDependency(vec![existing.clone()]))
            .await;
        harness.take_events();
        harness.take_commands();

        harness.access.set_content(&existing, "existing-v2");
        harness.access.set_content(&added, "added-v1");
        harness
            .apply(MatrixInput::UpstreamInvalidation {
                invalidates: vec![existing.clone(), added.clone()],
                opaque: 42,
            })
            .await;

        assert_eq!(harness.take_commands(), vec![watch(&added)]);
        let events = harness.take_events();
        assert_eq!(events.len(), 1);
        assert_upstream_update(
            &events[0],
            &[
                (&existing, ExpectedSnapshot::Content("existing-v2")),
                (&added, ExpectedSnapshot::Content("added-v1")),
            ],
            &[existing.clone(), added.clone()],
            42,
        );
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "uses the host filesystem watcher; CI runs real_fs_* explicitly"]
    async fn real_fs_sync_dependency_updates_and_readds_dependencies() {
        let mut harness = RealFsHarness::new();
        let first = harness.write("sync-first.typ", "first-v1");
        let second = harness.write("sync-second.typ", "second-v1");
        let third = harness.write("sync-third.typ", "third-v1");

        harness.sync(&[first.clone(), second.clone()]);
        harness
            .expect_update_all(
                true,
                &[
                    (&first, ExpectedSnapshot::Content("first-v1")),
                    (&second, ExpectedSnapshot::Content("second-v1")),
                ],
            )
            .await;

        harness.write_path(&first, "first-v2");
        harness
            .expect_update(&first, false, ExpectedSnapshot::Content("first-v2"))
            .await;

        harness.sync(std::slice::from_ref(&first));

        harness.sync(&[first.clone(), third.clone()]);
        harness
            .expect_update(&third, true, ExpectedSnapshot::Content("third-v1"))
            .await;
        harness.settle().await;
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "uses the host filesystem watcher; CI runs real_fs_* explicitly"]
    async fn real_fs_modify_unwatched_and_multi_file_updates() {
        let mut harness = RealFsHarness::new();
        let watched = harness.write("watched.typ", "watched-v1");
        let other = harness.write("other.typ", "other-v1");
        let unwatched = harness.write("unwatched.typ", "unwatched-v1");

        harness.sync(&[watched.clone(), other.clone()]);
        harness
            .expect_update_all(
                true,
                &[
                    (&watched, ExpectedSnapshot::Content("watched-v1")),
                    (&other, ExpectedSnapshot::Content("other-v1")),
                ],
            )
            .await;

        harness.write_path(&watched, "watched-v2");
        harness
            .expect_update(&watched, false, ExpectedSnapshot::Content("watched-v2"))
            .await;

        harness.drain_events();
        harness.write_path(&unwatched, "unwatched-v2");
        harness.expect_no_update(&unwatched).await;

        harness.write_path(&watched, "watched-v3");
        harness.write_path(&other, "other-v2");
        harness
            .expect_update(&watched, false, ExpectedSnapshot::Content("watched-v3"))
            .await;
        harness
            .expect_update(&other, false, ExpectedSnapshot::Content("other-v2"))
            .await;
        harness.settle().await;
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "uses the host filesystem watcher; CI runs real_fs_* explicitly"]
    async fn real_fs_remove_rename_away_and_readd_dependencies() {
        let mut harness = RealFsHarness::new();
        let remove = harness.write("remove.typ", "remove-v1");
        let rename = harness.write("rename-away.typ", "rename-v1");
        let renamed = harness.path("renamed-away.typ");

        harness.sync(&[remove.clone(), rename.clone()]);
        harness
            .expect_update_all(
                true,
                &[
                    (&remove, ExpectedSnapshot::Content("remove-v1")),
                    (&rename, ExpectedSnapshot::Content("rename-v1")),
                ],
            )
            .await;

        harness.remove(&remove);
        harness
            .expect_update(&remove, false, ExpectedSnapshot::NotFound)
            .await;

        harness.write_path(&remove, "remove-v2");
        harness.sync(&[remove.clone(), rename.clone()]);
        harness
            .expect_update(&remove, true, ExpectedSnapshot::Content("remove-v2"))
            .await;

        harness.rename(&rename, &renamed);
        harness
            .expect_update(&rename, false, ExpectedSnapshot::NotFound)
            .await;
        harness.settle().await;
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "uses the host filesystem watcher; CI runs real_fs_* explicitly"]
    async fn real_fs_atomic_replace_empty_missing_and_recovery() {
        let mut harness = RealFsHarness::new();
        let atomic = harness.write("atomic.typ", "atomic-v1");
        let empty = harness.write("empty.typ", "stable");
        let missing = harness.write("missing.typ", "stable");
        let recovery = harness.write("recovery.typ", "stable");

        harness.sync(&[
            atomic.clone(),
            empty.clone(),
            missing.clone(),
            recovery.clone(),
        ]);
        harness
            .expect_update_all(
                true,
                &[
                    (&atomic, ExpectedSnapshot::Content("atomic-v1")),
                    (&empty, ExpectedSnapshot::Content("stable")),
                    (&missing, ExpectedSnapshot::Content("stable")),
                    (&recovery, ExpectedSnapshot::Content("stable")),
                ],
            )
            .await;

        let atomic_tmp = harness.write("atomic.tmp", "atomic-v2");
        harness.rename(&atomic_tmp, &atomic);
        harness
            .expect_update(&atomic, false, ExpectedSnapshot::Content("atomic-v2"))
            .await;

        harness.write_path(&empty, "");
        harness
            .expect_update(&empty, false, ExpectedSnapshot::Content(""))
            .await;

        harness.remove(&missing);
        harness
            .expect_update(&missing, false, ExpectedSnapshot::NotFound)
            .await;

        harness.write_path(&recovery, "");
        harness.write_path(&recovery, "recovered");
        harness
            .expect_update(&recovery, false, ExpectedSnapshot::Content("recovered"))
            .await;
        harness.settle().await;
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore = "uses the host filesystem watcher; CI runs real_fs_* explicitly"]
    async fn real_fs_upstream_invalidation_refreshes_watches() {
        let mut harness = RealFsHarness::new();
        let existing = harness.write("upstream-existing.typ", "existing-v1");
        let added = harness.write("upstream-added.typ", "added-v1");

        harness.sync(std::slice::from_ref(&existing));
        harness
            .expect_update(&existing, true, ExpectedSnapshot::Content("existing-v1"))
            .await;

        harness.write_path(&existing, "existing-v2");
        harness.upstream(&[existing.clone(), added.clone()], 7);
        harness
            .expect_upstream(&existing, ExpectedSnapshot::Content("existing-v2"), 7)
            .await;
        harness.write_path(&added, "added-v2");
        harness
            .expect_update(&added, false, ExpectedSnapshot::Content("added-v2"))
            .await;
        harness.settle().await;
    }

    #[derive(Debug, Clone, Copy)]
    enum ExpectedSnapshot<'a> {
        Content(&'a str),
        NotFound,
        Other,
    }

    fn test_path(name: &str) -> ImmutPath {
        Arc::from(
            PathBuf::from("/tinymist-notify-actor-test")
                .join(name)
                .into_boxed_path(),
        )
    }

    fn notify_event(kind: notify::EventKind, paths: Vec<ImmutPath>) -> notify::Event {
        paths
            .into_iter()
            .fold(notify::Event::new(kind), |event, path| {
                event.add_path(path.as_ref().to_path_buf())
            })
    }

    fn modify_data_event() -> notify::EventKind {
        notify::EventKind::Modify(ModifyKind::Data(DataChange::Content))
    }

    fn content_snapshot(content: &str) -> FileSnapshot {
        Ok::<Bytes, FileError>(Bytes::from_string(content.to_owned())).into()
    }

    fn watch(path: &ImmutPath) -> FakeWatchCommand {
        FakeWatchCommand::Watch(path.as_ref().to_path_buf())
    }

    fn unwatch(path: &ImmutPath) -> FakeWatchCommand {
        FakeWatchCommand::Unwatch(path.as_ref().to_path_buf())
    }

    fn assert_update(
        event: &FilesystemEvent,
        expected_is_sync: bool,
        expected: &[(&ImmutPath, ExpectedSnapshot<'_>)],
    ) {
        let FilesystemEvent::Update(changeset, is_sync) = event else {
            panic!("expected update event, got {event:?}");
        };

        assert_eq!(*is_sync, expected_is_sync);
        assert_changeset(changeset, expected);
    }

    struct RealFsHarness {
        _dir: tempfile::TempDir,
        sender: mpsc::UnboundedSender<NotifyMessage>,
        events_recv: mpsc::UnboundedReceiver<FilesystemEvent>,
        handle: tokio::task::JoinHandle<()>,
    }

    impl RealFsHarness {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("tempdir should be created");
            let (sender, inbox) = mpsc::unbounded_channel();
            let (events_send, events_recv) = mpsc::unbounded_channel();
            let handle = spawn_watch_deps(inbox, move |event| {
                events_send
                    .send(event)
                    .expect("real watcher event receiver should stay open");
            });

            Self {
                _dir: dir,
                sender,
                events_recv,
                handle,
            }
        }

        fn path(&self, name: &str) -> ImmutPath {
            Arc::from(self._dir.path().join(name).into_boxed_path())
        }

        fn write(&self, name: &str, content: &str) -> ImmutPath {
            let path = self.path(name);
            self.write_path(&path, content);
            path
        }

        fn write_path(&self, path: &ImmutPath, content: &str) {
            std::fs::write(path.as_ref(), content).expect("temp file should be written");
        }

        fn remove(&self, path: &ImmutPath) {
            std::fs::remove_file(path.as_ref()).expect("temp file should be removed");
        }

        fn rename(&self, from: &ImmutPath, to: &ImmutPath) {
            std::fs::rename(from.as_ref(), to.as_ref()).expect("temp file should be renamed");
        }

        fn sync(&self, paths: &[ImmutPath]) {
            self.sender
                .send(NotifyMessage::SyncDependency(Box::new(paths.to_vec())))
                .expect("sync dependency send should succeed");
        }

        fn upstream(&self, invalidates: &[ImmutPath], opaque: usize) {
            self.sender
                .send(NotifyMessage::UpstreamUpdate(UpstreamUpdateEvent {
                    invalidates: invalidates.to_vec(),
                    opaque: Box::new(opaque),
                }))
                .expect("upstream update send should succeed");
        }

        async fn expect_update(
            &mut self,
            expected_path: &ImmutPath,
            expected_is_sync: bool,
            expected: ExpectedSnapshot<'_>,
        ) {
            self.expect_event(
                || {
                    format!(
                        "update path={expected_path:?}, is_sync={expected_is_sync}, snapshot={expected:?}"
                    )
                },
                |event| update_contains(event, expected_path, expected_is_sync, expected),
            )
            .await;
        }

        async fn expect_update_all(
            &mut self,
            expected_is_sync: bool,
            expected: &[(&ImmutPath, ExpectedSnapshot<'_>)],
        ) {
            self.expect_event(
                || format!("update is_sync={expected_is_sync}, snapshots={expected:?}"),
                |event| update_contains_all(event, expected_is_sync, expected),
            )
            .await;
        }

        async fn expect_upstream(
            &mut self,
            expected_path: &ImmutPath,
            expected: ExpectedSnapshot<'_>,
            expected_opaque: usize,
        ) {
            self.expect_event(
                || format!("upstream path={expected_path:?}, snapshot={expected:?}"),
                |event| upstream_contains(event, expected_path, expected, expected_opaque),
            )
            .await;
        }

        async fn expect_no_update(&mut self, expected_path: &ImmutPath) {
            let res = tokio::time::timeout(std::time::Duration::from_millis(250), async {
                loop {
                    let event = self
                        .events_recv
                        .recv()
                        .await
                        .expect("real watcher event sender should stay open");
                    if update_mentions_path(&event, expected_path) {
                        panic!("unexpected real watcher update for {expected_path:?}: {event:?}");
                    }
                }
            })
            .await;
            assert!(
                res.is_err(),
                "no-update wait should end by timeout, not by matching an event"
            );
        }

        async fn expect_event(
            &mut self,
            description: impl Fn() -> String,
            mut matches: impl FnMut(&FilesystemEvent) -> bool,
        ) {
            let mut last_event = None;
            let res = tokio::time::timeout(std::time::Duration::from_secs(3), async {
                loop {
                    let event = self
                        .events_recv
                        .recv()
                        .await
                        .expect("real watcher event sender should stay open");
                    if matches(&event) {
                        return;
                    }
                    last_event = Some(format!("{event:?}"));
                }
            })
            .await;

            if res.is_err() {
                panic!(
                    "timed out waiting for real watcher {}; last event: {}",
                    description(),
                    last_event.unwrap_or_else(|| "<none>".to_owned())
                );
            }
        }

        fn drain_events(&mut self) {
            while self.events_recv.try_recv().is_ok() {}
        }

        async fn settle(self) {
            self.sender
                .send(NotifyMessage::Settle)
                .expect("settle send should succeed");
            tokio::time::timeout(std::time::Duration::from_millis(500), self.handle)
                .await
                .expect("production notify actor did not settle")
                .expect("production notify actor task failed");
        }
    }

    fn update_contains(
        event: &FilesystemEvent,
        expected_path: &ImmutPath,
        expected_is_sync: bool,
        expected: ExpectedSnapshot<'_>,
    ) -> bool {
        let FilesystemEvent::Update(changeset, is_sync) = event else {
            return false;
        };
        *is_sync == expected_is_sync && changeset_contains(changeset, expected_path, expected)
    }

    fn update_contains_all(
        event: &FilesystemEvent,
        expected_is_sync: bool,
        expected: &[(&ImmutPath, ExpectedSnapshot<'_>)],
    ) -> bool {
        let FilesystemEvent::Update(changeset, is_sync) = event else {
            return false;
        };
        *is_sync == expected_is_sync
            && expected
                .iter()
                .all(|(path, snapshot)| changeset_contains(changeset, path, *snapshot))
    }

    fn upstream_contains(
        event: &FilesystemEvent,
        expected_path: &ImmutPath,
        expected: ExpectedSnapshot<'_>,
        expected_opaque: usize,
    ) -> bool {
        let FilesystemEvent::UpstreamUpdate {
            changeset,
            upstream_event: Some(upstream_event),
        } = event
        else {
            return false;
        };

        upstream_event
            .opaque
            .downcast_ref::<usize>()
            .is_some_and(|opaque| *opaque == expected_opaque)
            && changeset_contains(changeset, expected_path, expected)
    }

    fn update_mentions_path(event: &FilesystemEvent, expected_path: &ImmutPath) -> bool {
        let FilesystemEvent::Update(changeset, ..) = event else {
            return false;
        };

        changeset
            .inserts
            .iter()
            .any(|(path, _)| path == expected_path)
            || changeset.removes.iter().any(|path| path == expected_path)
    }

    fn changeset_contains(
        changeset: &FileChangeSet,
        expected_path: &ImmutPath,
        expected: ExpectedSnapshot<'_>,
    ) -> bool {
        changeset.inserts.iter().any(|(path, snapshot)| {
            path == expected_path && snapshot_matches(snapshot, expected_path, expected)
        })
    }

    fn snapshot_matches(
        snapshot: &FileSnapshot,
        expected_path: &ImmutPath,
        expected: ExpectedSnapshot<'_>,
    ) -> bool {
        match expected {
            ExpectedSnapshot::Content(content) => snapshot
                .content()
                .is_ok_and(|bytes| bytes.as_slice() == content.as_bytes()),
            ExpectedSnapshot::NotFound => {
                let Err(err) = snapshot.as_ref() else {
                    return false;
                };
                let FileError::NotFound(actual_path) = err.as_ref() else {
                    return false;
                };
                actual_path.as_path() == expected_path.as_ref()
            }
            ExpectedSnapshot::Other => snapshot
                .as_ref()
                .is_err_and(|err| matches!(err.as_ref(), FileError::Other(_))),
        }
    }

    fn assert_upstream_update(
        event: &FilesystemEvent,
        expected: &[(&ImmutPath, ExpectedSnapshot<'_>)],
        expected_invalidates: &[ImmutPath],
        expected_opaque: usize,
    ) {
        let FilesystemEvent::UpstreamUpdate {
            changeset,
            upstream_event: Some(upstream_event),
        } = event
        else {
            panic!("expected upstream update event, got {event:?}");
        };

        assert_changeset(changeset, expected);
        assert_eq!(upstream_event.invalidates, expected_invalidates);
        assert_eq!(
            upstream_event
                .opaque
                .downcast_ref::<usize>()
                .copied()
                .expect("opaque payload should be usize"),
            expected_opaque
        );
    }

    fn assert_changeset(
        changeset: &FileChangeSet,
        expected: &[(&ImmutPath, ExpectedSnapshot<'_>)],
    ) {
        assert_eq!(changeset.removes, Vec::<ImmutPath>::new());
        assert_eq!(changeset.inserts.len(), expected.len());

        for ((actual_path, actual_snapshot), (expected_path, expected_snapshot)) in
            changeset.inserts.iter().zip(expected)
        {
            assert_eq!(actual_path, *expected_path);
            assert_snapshot(expected_path, actual_snapshot, *expected_snapshot);
        }
    }

    fn assert_snapshot(path: &ImmutPath, snapshot: &FileSnapshot, expected: ExpectedSnapshot<'_>) {
        match expected {
            ExpectedSnapshot::Content(content) => {
                let bytes = snapshot.content().expect("expected file content");
                assert_eq!(bytes.as_slice(), content.as_bytes());
            }
            ExpectedSnapshot::NotFound => {
                let Err(err) = snapshot.as_ref() else {
                    panic!("expected not found snapshot for {path:?}");
                };
                let FileError::NotFound(actual_path) = err.as_ref() else {
                    panic!("expected not found snapshot for {path:?}, got {err:?}");
                };
                assert_eq!(actual_path.as_path(), path.as_ref());
            }
            ExpectedSnapshot::Other => {
                let Err(err) = snapshot.as_ref() else {
                    panic!("expected other-error snapshot for {path:?}");
                };
                assert!(
                    matches!(err.as_ref(), FileError::Other(_)),
                    "expected other-error snapshot for {path:?}, got {err:?}"
                );
            }
        }
    }
}
