use core::fmt;
use std::path::Path;

use rpds::RedBlackTreeMapSync;
use typst::diag::{FileError, FileResult};

use crate::{AccessModel, Bytes, ImmutPath};

/// internal representation of [`NotifyFile`]
#[derive(Debug, Clone)]
struct NotifyFileRepr {
    mtime: crate::Time,
    content: Bytes,
}

/// A file snapshot that is notified by some external source
///
/// Note: The error is boxed to avoid large stack size
#[derive(Clone)]
pub struct FileSnapshot(Result<NotifyFileRepr, Box<FileError>>);

impl fmt::Debug for FileSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.as_ref() {
            Ok(v) => f
                .debug_struct("FileSnapshot")
                .field("mtime", &v.mtime)
                .field(
                    "content",
                    &FileContent {
                        len: v.content.len(),
                    },
                )
                .finish(),
            Err(e) => f.debug_struct("FileSnapshot").field("error", &e).finish(),
        }
    }
}

impl FileSnapshot {
    /// Access the internal data of the file snapshot
    #[inline]
    #[track_caller]
    fn retrieve<'a, T>(&'a self, f: impl FnOnce(&'a NotifyFileRepr) -> T) -> FileResult<T> {
        self.0.as_ref().map(f).map_err(|e| *e.clone())
    }

    /// mtime of the file
    pub fn mtime(&self) -> FileResult<&crate::Time> {
        self.retrieve(|e| &e.mtime)
    }

    /// content of the file
    pub fn content(&self) -> FileResult<&Bytes> {
        self.retrieve(|e| &e.content)
    }

    /// Whether the related file is a file
    pub fn is_file(&self) -> FileResult<bool> {
        self.retrieve(|_| true)
    }
}

/// Convenient function to create a [`FileSnapshot`] from tuple
impl From<FileResult<(crate::Time, Bytes)>> for FileSnapshot {
    fn from(result: FileResult<(crate::Time, Bytes)>) -> Self {
        Self(
            result
                .map(|(mtime, content)| NotifyFileRepr { mtime, content })
                .map_err(Box::new),
        )
    }
}

/// A set of changes to the filesystem.
///
/// The correct order of applying changes is:
/// 1. Remove files
/// 2. Upsert (Insert or Update) files
#[derive(Debug, Clone, Default)]
pub struct FileChangeSet {
    /// Files to remove
    pub removes: Vec<ImmutPath>,
    /// Files to insert or update
    pub inserts: Vec<(ImmutPath, FileSnapshot)>,
}

impl FileChangeSet {
    /// Create a new empty changeset
    pub fn is_empty(&self) -> bool {
        self.inserts.is_empty() && self.removes.is_empty()
    }

    /// Create a new changeset with removing files
    pub fn new_removes(removes: Vec<ImmutPath>) -> Self {
        Self {
            removes,
            inserts: vec![],
        }
    }

    /// Create a new changeset with inserting files
    pub fn new_inserts(inserts: Vec<(ImmutPath, FileSnapshot)>) -> Self {
        Self {
            removes: vec![],
            inserts,
        }
    }

    /// Utility function to insert a possible file to insert or update
    pub fn may_insert(&mut self, v: Option<(ImmutPath, FileSnapshot)>) {
        if let Some(v) = v {
            self.inserts.push(v);
        }
    }

    /// Utility function to insert multiple possible files to insert or update
    pub fn may_extend(&mut self, v: Option<impl Iterator<Item = (ImmutPath, FileSnapshot)>>) {
        if let Some(v) = v {
            self.inserts.extend(v);
        }
    }
}

/// A memory event that is notified by some external source
#[derive(Debug)]
pub enum MemoryEvent {
    /// Reset all dependencies and update according to the given changeset
    ///
    /// We have not provided a way to reset all dependencies without updating
    /// yet, but you can create a memory event with empty changeset to achieve
    /// this:
    ///
    /// ```
    /// use tinymist_vfs::notify::{MemoryEvent, FileChangeSet};
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

/// A message that is sent to some file watcher
#[derive(Debug)]
pub enum NotifyMessage {
    /// Oettle the watching
    Settle,
    /// Overrides all dependencies
    SyncDependency(Vec<ImmutPath>),
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

impl<M: AccessModel> NotifyAccessModel<M> {
    /// Create a new notify access model
    pub fn new(inner: M) -> Self {
        Self {
            files: RedBlackTreeMapSync::default(),
            inner,
        }
    }

    /// Notify the access model with a filesystem event
    pub fn notify(&mut self, event: FilesystemEvent) {
        match event {
            FilesystemEvent::UpstreamUpdate { changeset, .. }
            | FilesystemEvent::Update(changeset) => {
                for path in changeset.removes {
                    self.files.remove_mut(&path);
                }

                for (path, contents) in changeset.inserts {
                    self.files.insert_mut(path, contents);
                }
            }
        }
    }
}

impl<M: AccessModel> AccessModel for NotifyAccessModel<M> {
    fn is_file(&self, src: &Path) -> FileResult<bool> {
        if let Some(entry) = self.files.get(src) {
            return entry.is_file();
        }

        self.inner.is_file(src)
    }

    fn content(&self, src: &Path) -> FileResult<Bytes> {
        if let Some(entry) = self.files.get(src) {
            return entry.content().cloned();
        }

        self.inner.content(src)
    }
}

#[derive(Debug)]
#[allow(dead_code)]
struct FileContent {
    len: usize,
}
