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

use std::collections::HashMap;

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use typst::diag::{EcoString, FileError, FileResult};

use crate::vfs::{
    notify::{FileChangeSet, FileSnapshot, FilesystemEvent, NotifyMessage, UpstreamUpdateEvent},
    system::SystemAccessModel,
    PathAccessModel,
};
use tinymist_std::ImmutPath;

type WatcherPair = (RecommendedWatcher, mpsc::UnboundedReceiver<NotifyEvent>);
type NotifyEvent = notify::Result<notify::Event>;
type FileEntry = (/* key */ ImmutPath, /* value */ FileSnapshot);

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
    /// Previous metadata of the file.
    prev_meta: FileResult<std::fs::Metadata>,
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
    /// We concrete the access model to `SystemAccessModel` for now.
    inner: SystemAccessModel,

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
    watcher: Option<WatcherPair>,
}

impl<F: FnMut(FilesystemEvent) + Send + Sync> NotifyActor<F> {
    /// Create a new actor.
    pub fn new(interrupted_by_events: F) -> Self {
        let (undetermined_send, undetermined_recv) = mpsc::unbounded_channel();
        let (watcher_sender, watcher_receiver) = mpsc::unbounded_channel();
        let watcher = log_notify_error(
            RecommendedWatcher::new(
                move |event| {
                    let res = watcher_sender.send(event);
                    if let Err(err) = res {
                        log::warn!("error to send event: {err}");
                    }
                },
                Config::default(),
            ),
            "failed to create watcher",
        );

        NotifyActor {
            inner: SystemAccessModel,
            // we start from 1 to distinguish from 0 (default value)
            lifetime: 1,
            logical_tick: 1,

            interrupted_by_events,

            undetermined_send,
            undetermined_recv,

            watched_entries: HashMap::new(),
            watcher: watcher.map(|it| (it, watcher_receiver)),
        }
    }

    /// Get the notify event from the watcher.
    async fn get_notify_event(watcher: &mut Option<WatcherPair>) -> Option<NotifyEvent> {
        match watcher {
            Some((_, watcher_receiver)) => watcher_receiver.recv().await,
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
                    log::info!("failed to get event, exiting...");
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
                    if let Some(changeset) = self.update_watches(&paths) {
                        (self.interrupted_by_events)(FilesystemEvent::Update(changeset));
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
    fn update_watches(&mut self, paths: &[ImmutPath]) -> Option<FileChangeSet> {
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
        for path in paths.iter() {
            let mut contained = false;
            // Update or insert the entry with the new lifetime.
            let entry = self
                .watched_entries
                .entry(path.clone())
                .and_modify(|watch_entry| {
                    contained = true;
                    watch_entry.lifetime = self.lifetime;
                    watch_entry.seen = true;
                })
                .or_insert_with(|| WatchedEntry {
                    lifetime: self.lifetime,
                    watching: false,
                    seen: true,
                    state: WatchState::Stable,
                    prev: None,
                    prev_meta: Err(FileError::Other(Some(EcoString::from("_not-init_")))),
                });

            // Update in-memory metadata for now.
            let meta = path.metadata().map_err(|e| FileError::from_io(e, path));

            if let Some((watcher, _)) = &mut self.watcher {
                // Case1. meta = Err(..) We cannot get the metadata successfully, so we
                // are okay to ignore this file for watching.
                //
                // Case2. meta = Ok(..) Watch the file if it's not watched.
                if meta
                    .as_ref()
                    .is_ok_and(|meta| !meta.is_dir() && (!contained || !entry.watching))
                {
                    log::debug!("watching {path:?}");
                    entry.watching = log_notify_error(
                        watcher.watch(path.as_ref(), RecursiveMode::NonRecursive),
                        "failed to watch",
                    )
                    .is_some();
                }

                changeset.may_insert(self.notify_entry_update(path.clone(), Some(meta)));
            } else {
                let watched = self.inner.content(path);
                changeset.inserts.push((path.clone(), watched.into()));
            }
        }

        // Remove old entries.
        // Note: since we have increased the lifetime, it is safe to remove the
        // old entries after updating the watched entries.
        self.watched_entries.retain(|path, entry| {
            if !entry.seen && entry.watching {
                log::debug!("unwatch {path:?}");
                if let Some(watcher) = &mut self.watcher {
                    log_notify_error(watcher.0.unwatch(path), "failed to unwatch");
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
            changeset.may_insert(self.notify_entry_update(path.as_path().into(), None));
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
                    log_notify_error(watcher.0.unwatch(path), "failed to unwatch");
                }
                entry.watching = false;
            }
        }

        // Send file updates.
        if !changeset.is_empty() {
            (self.interrupted_by_events)(FilesystemEvent::Update(changeset));
        }
    }

    /// Notify any update of the file entry
    fn notify_entry_update(
        &mut self,
        path: ImmutPath,
        meta: Option<FileResult<std::fs::Metadata>>,
    ) -> Option<FileEntry> {
        let mut meta =
            meta.unwrap_or_else(|| path.metadata().map_err(|e| FileError::from_io(e, &path)));

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

        std::mem::swap(&mut entry.prev_meta, &mut meta);
        let prev_meta = meta;
        let next_meta = &entry.prev_meta;

        let meta = match (prev_meta, next_meta) {
            (Err(prev), Err(next)) => {
                if prev != *next {
                    return Some((path.clone(), FileSnapshot::from(Err(next.clone()))));
                }
                return None;
            }
            // todo: check correctness
            (Ok(..), Err(next)) => {
                // Invalidate the entry content
                entry.prev = None;

                return Some((path.clone(), FileSnapshot::from(Err(next.clone()))));
            }
            (_, Ok(meta)) => meta,
        };

        if !meta.file_type().is_file() {
            return None;
        }

        // Check meta, path, and content

        let mut file = FileSnapshot::from(self.inner.content(&path));

        // Check state in fast path: compare state, return None on not sending
        // the file change
        match (entry.prev.as_deref(), file.as_mut()) {
            // update the content of the entry in the following cases:
            // + Case 1: previous content is clear
            // + Case 2: previous content is not clear but some error, and the
            // current content is ok
            (None, ..) | (Some(Err(..)), Ok(..)) => {}
            // Meet some error currently
            (Some(..), Err(err)) => match &mut entry.state {
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
        if reserved < std::time::Duration::from_millis(50) {
            let send = self.undetermined_send.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(50) - reserved).await;
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
                    (self.interrupted_by_events)(FilesystemEvent::Update(changeset));
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

pub async fn watch_deps(
    inbox: mpsc::UnboundedReceiver<NotifyMessage>,
    interrupted_by_events: impl FnMut(FilesystemEvent) + Send + Sync + 'static,
) {
    log::debug!("start watching files...");
    // Watch messages to notify
    tokio::spawn(NotifyActor::new(interrupted_by_events).run(inbox));
}
