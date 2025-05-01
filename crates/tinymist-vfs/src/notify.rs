use core::fmt;
use std::path::Path;

use rpds::RedBlackTreeMapSync;
use typst::diag::FileResult;

use crate::{Bytes, FileChangeSet, FileSnapshot, ImmutPath, PathAccessModel};

/// A memory event that is notified by some external source
#[derive(Debug, Clone)]
pub enum MemoryEvent {
    /// Reset all dependencies and update according to the given changeset
    ///
    /// We have not provided a way to reset all dependencies without updating
    /// yet, but you can create a memory event with empty changeset to achieve
    /// this:
    ///
    /// ```
    /// use tinymist_vfs::{FileChangeSet, notify::MemoryEvent};
    /// let event = MemoryEvent::Sync(FileChangeSet::default());
    /// ```
    Sync(FileChangeSet),
    /// Update according to the given changeset
    Update(FileChangeSet),
}

/// A upstream update event that is notified by some external source.
///
/// This event is used to notify some file watcher to invalidate some files
/// before applying upstream changes. This is very important to make some atomic
/// changes.
#[derive(Debug)]
pub struct UpstreamUpdateEvent {
    /// Associated files that the event causes to invalidate
    pub invalidates: Vec<ImmutPath>,
    /// Opaque data that is passed to the file watcher
    pub opaque: Box<dyn std::any::Any + Send>,
}

/// Aggregated filesystem events from some file watcher
#[derive(Debug)]
pub enum FilesystemEvent {
    /// Update file system files according to the given changeset
    Update(FileChangeSet),
    /// See [`UpstreamUpdateEvent`]
    UpstreamUpdate {
        /// New changeset produced by invalidation
        changeset: FileChangeSet,
        /// The upstream event that causes the invalidation
        upstream_event: Option<UpstreamUpdateEvent>,
    },
}

impl FilesystemEvent {
    /// Splits the filesystem event into a changeset and an optional upstream
    /// event.
    pub fn split(self) -> (FileChangeSet, Option<UpstreamUpdateEvent>) {
        match self {
            FilesystemEvent::UpstreamUpdate {
                changeset,
                upstream_event,
            } => (changeset, upstream_event),
            FilesystemEvent::Update(changeset) => (changeset, None),
        }
    }
}

/// A trait implementing dependency getter.
pub trait NotifyDeps: fmt::Debug + Send + Sync {
    /// Gets the dependencies recorded in the world. It is a list of
    /// accessed file recorded during this revision, e.g. a single compilation
    /// or other compiler tasks.
    fn dependencies(&self, f: &mut dyn FnMut(&ImmutPath));
}

impl NotifyDeps for Vec<ImmutPath> {
    fn dependencies(&self, f: &mut dyn FnMut(&ImmutPath)) {
        for path in self.iter() {
            f(path);
        }
    }
}

/// A message that is sent to some file watcher
#[derive(Debug)]
pub enum NotifyMessage {
    /// Oettle the watching
    Settle,
    /// Overrides all dependencies
    SyncDependency(Box<dyn NotifyDeps>),
    /// upstream invalidation This is very important to make some atomic changes
    ///
    /// Example:
    /// ```plain
    ///   /// Receive memory event
    ///   let event: MemoryEvent = retrieve();
    ///   let invalidates = event.invalidates();
    ///
    ///   /// Send memory change event to [`NotifyActor`]
    ///   let event = Box::new(event);
    ///   self.send(NotifyMessage::UpstreamUpdate{ invalidates, opaque: event });
    ///
    ///   /// Wait for [`NotifyActor`] to finish
    ///   let fs_event = self.fs_notify.block_receive();
    ///   let event: MemoryEvent = fs_event.opaque.downcast().unwrap();
    ///
    ///   /// Apply changes
    ///   self.lock();
    ///   update_memory(event);
    ///   apply_fs_changes(fs_event.changeset);
    ///   self.unlock();
    /// ```
    UpstreamUpdate(UpstreamUpdateEvent),
}

/// Provides notify access model which retrieves file system events and changes
/// from some notify backend.
///
/// It simply hold notified filesystem data in memory, but still have a fallback
/// access model, whose the typical underlying access model is
/// [`crate::system::SystemAccessModel`]
#[derive(Debug, Clone)]
pub struct NotifyAccessModel<M> {
    files: RedBlackTreeMapSync<ImmutPath, FileSnapshot>,
    /// The fallback access model when the file is not notified ever.
    pub inner: M,
}

impl<M: PathAccessModel> NotifyAccessModel<M> {
    /// Create a new notify access model
    pub fn new(inner: M) -> Self {
        Self {
            files: RedBlackTreeMapSync::default(),
            inner,
        }
    }

    /// Notify the access model with a filesystem event
    pub fn notify(&mut self, changeset: FileChangeSet) {
        for path in changeset.removes {
            self.files.remove_mut(&path);
        }

        for (path, contents) in changeset.inserts {
            self.files.insert_mut(path, contents);
        }
    }
}

impl<M: PathAccessModel> PathAccessModel for NotifyAccessModel<M> {
    #[inline]
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn content(&self, src: &Path) -> FileResult<Bytes> {
        if let Some(entry) = self.files.get(src) {
            return entry.content().cloned();
        }

        self.inner.content(src)
    }
}
