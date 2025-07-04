//! upstream of following files <https://github.com/rust-lang/rust-analyzer/tree/master/crates/vfs>
//!   ::path_interner.rs -> path_interner.rs

/// Provides ProxyAccessModel that makes access to JavaScript objects for
/// browser compilation.
#[cfg(feature = "browser")]
pub mod browser;

/// Provides SystemAccessModel that makes access to the local file system for
/// system compilation.
#[cfg(feature = "system")]
pub mod system;

/// Provides dummy access model.
///
/// Note: we can still perform compilation with dummy access model, since
/// [`Vfs`] will make a overlay access model over the provided dummy access
/// model.
pub mod dummy;

/// Provides snapshot models
pub mod snapshot;
pub use snapshot::*;
use tinymist_std::hash::{FxDashMap, FxHashMap};

/// Provides notify access model which retrieves file system events and changes
/// from some notify backend.
pub mod notify;
pub use notify::{FilesystemEvent, MemoryEvent};
/// Provides overlay access model which allows to shadow the underlying access
/// model with memory contents.
pub mod overlay;
/// Provides resolve access model.
pub mod resolve;
/// Provides trace access model which traces the underlying access model.
pub mod trace;
mod utils;

mod path_mapper;
pub use path_mapper::{PathResolution, RootResolver, WorkspaceResolution, WorkspaceResolver};

use core::fmt;
use std::num::NonZeroUsize;
use std::sync::OnceLock;
use std::{path::Path, sync::Arc};

use ecow::EcoVec;
use parking_lot::Mutex;
use rpds::RedBlackTreeMapSync;
use typst::diag::{FileError, FileResult};
use typst::foundations::Dict;
use typst::syntax::Source;
use typst::utils::LazyHash;

use crate::notify::NotifyAccessModel;
use crate::overlay::OverlayAccessModel;
use crate::resolve::ResolveAccessModel;

pub use tinymist_std::time::Time;
pub use tinymist_std::ImmutPath;
pub use typst::foundations::Bytes;
pub use typst::syntax::FileId;

/// Immutable prehashed reference to dictionary.
pub type ImmutDict = Arc<LazyHash<Dict>>;

/// A trait for accessing underlying file system.
///
/// This trait is simplified by [`Vfs`] and requires a minimal method set for
/// typst compilation.
pub trait PathAccessModel {
    /// Clears the cache of the access model.
    ///
    /// This is called when the vfs is reset. See [`Vfs`]'s reset method for
    /// more information.
    fn reset(&mut self) {}

    /// Returns the content of a file entry.
    fn content(&self, src: &Path) -> FileResult<Bytes>;
}

/// A trait for accessing underlying file system.
///
/// This trait is simplified by [`Vfs`] and requires a minimal method set for
/// typst compilation.
pub trait AccessModel {
    /// Clears the cache of the access model.
    ///
    /// This is called when the vfs is reset. See [`Vfs`]'s reset method for
    /// more information.
    fn reset(&mut self) {}

    /// Returns the content of a file entry.
    fn content(&self, src: FileId) -> (Option<ImmutPath>, FileResult<Bytes>);
}

type VfsPathAccessModel<M> = OverlayAccessModel<ImmutPath, NotifyAccessModel<M>>;
/// we add notify access model here since notify access model doesn't introduce
/// overheads by our observation
type VfsAccessModel<M> = OverlayAccessModel<FileId, ResolveAccessModel<VfsPathAccessModel<M>>>;

/// A trait to perform file system query.
pub trait FsProvider {
    /// Gets the file path corresponding to the given `id`.
    fn file_path(&self, id: FileId) -> FileResult<PathResolution>;
    /// Gets the file content corresponding to the given `id`.
    fn read(&self, id: FileId) -> FileResult<Bytes>;
    /// Gets the source code corresponding to the given `id`. It is preferred to
    /// be used for source files so that parsing is reused across editions.
    fn read_source(&self, id: FileId) -> FileResult<Source>;
}

struct SourceEntry {
    last_accessed_rev: NonZeroUsize,
    source: FileResult<Source>,
}

#[derive(Default)]
struct SourceIdShard {
    last_accessed_rev: usize,
    recent_source: Option<Source>,
    sources: FxHashMap<Bytes, SourceEntry>,
}

/// A source cache shared across VFS.
#[derive(Default, Clone)]
pub struct SourceCache {
    /// The cache entries for each paths
    cache_entries: Arc<FxDashMap<FileId, SourceIdShard>>,
}

impl SourceCache {
    /// Evicts cache, given a current revision `curr`, and a threshold. The too
    /// old cache entries will be evicted from the cache.
    pub fn evict(&self, curr: NonZeroUsize, threshold: usize) {
        self.cache_entries.retain(|_, shard| {
            let diff = curr.get().saturating_sub(shard.last_accessed_rev);
            if diff > threshold {
                return false;
            }

            shard.sources.retain(|_, entry| {
                let diff = curr.get().saturating_sub(entry.last_accessed_rev.get());
                diff <= threshold
            });

            true
        });
    }
}

/// Creates a new `Vfs` harnessing over the given `access_model` specific for
/// `reflexo_world::CompilerWorld`. With vfs, we can minimize the
/// implementation overhead for [`AccessModel`] trait.
pub struct Vfs<M: PathAccessModel + Sized> {
    source_cache: SourceCache,
    managed: Arc<Mutex<EntryMap>>,
    paths: Arc<Mutex<PathMap>>,
    revision: NonZeroUsize,
    // access_model: TraceAccessModel<VfsAccessModel<M>>,
    /// The wrapped access model.
    access_model: VfsAccessModel<M>,
}

impl<M: PathAccessModel + Sized> fmt::Debug for Vfs<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vfs")
            .field("revision", &self.revision)
            .field("managed", &self.managed.lock().entries.size())
            .field("paths", &self.paths.lock().paths.len())
            .finish()
    }
}

impl<M: PathAccessModel + Clone + Sized> Vfs<M> {
    /// Gets current revision of the vfs.
    pub fn revision(&self) -> NonZeroUsize {
        self.revision
    }

    /// Performs snapshot with sharing cache and managed resource.
    pub fn snapshot(&self) -> Self {
        Self {
            revision: self.revision,
            source_cache: self.source_cache.clone(),
            managed: self.managed.clone(),
            paths: self.paths.clone(),
            access_model: self.access_model.clone(),
        }
    }

    /// Performs snapshot with sharing cache, but not the resources.
    pub fn fork(&self) -> Self {
        Self {
            // todo: it is not correct to merely share source cache.
            source_cache: self.source_cache.clone(),
            managed: Arc::new(Mutex::new(EntryMap::default())),
            paths: Arc::new(Mutex::new(PathMap::default())),
            revision: NonZeroUsize::new(2).expect("initial revision is 2"),
            access_model: self.access_model.clone(),
        }
    }

    /// Detects whether the vfs is clean respecting a given revision and
    /// `file_ids`.
    pub fn is_clean_compile(&self, rev: usize, file_ids: &[FileId]) -> bool {
        let mut m = self.managed.lock();
        for id in file_ids {
            let Some(entry) = m.entries.get_mut(id) else {
                log::debug!("Vfs(dirty, {id:?}): file id not found");
                return false;
            };
            if entry.changed_at > rev {
                log::debug!("Vfs(dirty, {id:?}): rev {rev:?} => {:?}", entry.changed_at);
                return false;
            }
            log::debug!(
                "Vfs(clean, {id:?}, rev={rev}, changed_at={})",
                entry.changed_at
            );
        }
        true
    }
}

impl<M: PathAccessModel + Sized> Vfs<M> {
    /// Creates a new `Vfs` with a given `access_model`.
    ///
    /// Retrieving an [`AccessModel`], it will further wrap the access model
    /// with [`OverlayAccessModel`] and [`NotifyAccessModel`]. This means that
    /// you don't need to implement:
    /// + overlay: allowing to shadow the underlying access model with memory
    ///   contents, which is useful for a limited execution environment and
    ///   instrumenting or overriding source files or packages.
    /// + notify: regards problems of synchronizing with the file system when
    ///   the vfs is watching the file system.
    ///
    /// See [`AccessModel`] for more information.
    pub fn new(resolver: Arc<dyn RootResolver + Send + Sync>, access_model: M) -> Self {
        let access_model = NotifyAccessModel::new(access_model);
        let access_model = OverlayAccessModel::new(access_model);
        let access_model = ResolveAccessModel {
            resolver,
            inner: access_model,
        };
        let access_model = OverlayAccessModel::new(access_model);

        // If you want to trace the access model, uncomment the following line
        // let access_model = TraceAccessModel::new(access_model);

        Self {
            source_cache: SourceCache::default(),
            managed: Arc::default(),
            paths: Arc::default(),
            revision: NonZeroUsize::new(2).expect("initial revision is 2"),
            access_model,
        }
    }

    /// Resets all state.
    pub fn reset_all(&mut self) {
        self.reset_access_model();
        self.reset_read();
        self.take_source_cache();
    }

    /// Resets access model.
    pub fn reset_access_model(&mut self) {
        self.access_model.reset();
    }

    /// Resets all read caches. This can happen when:
    /// - package paths are reconfigured.
    /// - The root of the workspace is switched.
    pub fn reset_read(&mut self) {
        self.managed = Arc::default();
        self.paths = Arc::default();
    }

    /// Clears the cache that is not touched for a long time.
    pub fn evict(&mut self, threshold: usize) {
        let mut m = self.managed.lock();
        let rev = self.revision.get();
        for (id, entry) in m.entries.clone().iter() {
            let entry_rev = entry.bytes.get().map(|e| e.1).unwrap_or_default();
            if entry_rev + threshold < rev {
                m.entries.remove_mut(id);
            }
        }
    }

    /// Takes source cache. It also cleans the cache in the current vfs.
    pub fn take_source_cache(&mut self) -> SourceCache {
        std::mem::take(&mut self.source_cache)
    }

    /// Takes source cache for sharing.
    pub fn clone_source_cache(&self) -> SourceCache {
        self.source_cache.clone()
    }

    /// Resolve the real path for a file id.
    pub fn file_path(&self, id: FileId) -> Result<PathResolution, FileError> {
        self.access_model.inner.resolver.path_for_id(id)
    }

    /// Get paths to all the shadowing paths in [`OverlayAccessModel`].
    pub fn shadow_paths(&self) -> Vec<ImmutPath> {
        self.access_model.inner.inner.file_paths()
    }

    /// Get paths to all the shadowing file ids in [`OverlayAccessModel`].
    ///
    /// The in memory untitled files can have no path so
    /// they only have file ids.
    pub fn shadow_ids(&self) -> Vec<FileId> {
        self.access_model.file_paths()
    }

    /// Returns the overall memory usage for the stored files.
    pub fn memory_usage(&self) -> usize {
        0
    }

    /// Obtains an object to revise. The object will update the original vfs
    /// when it is dropped.
    pub fn revise(&mut self) -> RevisingVfs<M> {
        let managed = self.managed.lock().clone();
        let paths = self.paths.lock().clone();
        let goal_revision = self.revision.checked_add(1).expect("revision overflowed");

        RevisingVfs {
            managed,
            paths,
            inner: self,
            goal_revision,
            view_changed: false,
        }
    }

    /// Obtains an object to display.
    pub fn display(&self) -> DisplayVfs<M> {
        DisplayVfs { inner: self }
    }

    /// Reads a file by id.
    pub fn read(&self, fid: FileId) -> FileResult<Bytes> {
        let bytes = self.managed.lock().slot(fid, |entry| entry.bytes.clone());

        self.read_content(&bytes, fid).clone()
    }

    /// Reads a source file by id. It is preferred to be used for source files
    /// so that parsing is reused across editions.
    pub fn source(&self, file_id: FileId) -> FileResult<Source> {
        let (bytes, source) = self
            .managed
            .lock()
            .slot(file_id, |entry| (entry.bytes.clone(), entry.source.clone()));

        let source = source.get_or_init(|| {
            let content = self
                .read_content(&bytes, file_id)
                .as_ref()
                .map_err(Clone::clone)?;

            let mut cache_entry = self.source_cache.cache_entries.entry(file_id).or_default();
            if let Some(source) = cache_entry.sources.get(content) {
                return source.source.clone();
            }

            let source = (|| {
                let prev = cache_entry.recent_source.clone();
                let content = from_utf8_or_bom(content).map_err(|_| FileError::InvalidUtf8)?;

                let next = match prev {
                    Some(mut prev) => {
                        prev.replace(content);
                        prev
                    }
                    None => Source::new(file_id, content.to_owned()),
                };

                let should_update = cache_entry.recent_source.is_none()
                    || cache_entry.last_accessed_rev < self.revision.get();
                if should_update {
                    cache_entry.recent_source = Some(next.clone());
                }

                Ok(next)
            })();

            let entry = cache_entry
                .sources
                .entry(content.clone())
                .or_insert_with(|| SourceEntry {
                    last_accessed_rev: self.revision,
                    source: source.clone(),
                });

            if entry.last_accessed_rev < self.revision {
                entry.last_accessed_rev = self.revision;
            }

            source
        });

        source.clone()
    }

    /// Reads and caches content of a file.
    fn read_content<'a>(&self, bytes: &'a BytesQuery, fid: FileId) -> &'a FileResult<Bytes> {
        &bytes
            .get_or_init(|| {
                let (path, content) = self.access_model.content(fid);
                if let Some(path) = path.as_ref() {
                    self.paths.lock().insert(path, fid, self.revision);
                }

                (path, self.revision.get(), content)
            })
            .2
    }
}

/// A display wrapper for [`Vfs`].
pub struct DisplayVfs<'a, M: PathAccessModel + Sized> {
    inner: &'a Vfs<M>,
}

impl<M: PathAccessModel + Sized> fmt::Debug for DisplayVfs<'_, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vfs")
            .field("revision", &self.inner.revision)
            .field("managed", &self.inner.managed.lock().display())
            .field("paths", &self.inner.paths.lock().display())
            .finish()
    }
}

/// A revising wrapper for [`Vfs`].
pub struct RevisingVfs<'a, M: PathAccessModel + Sized> {
    inner: &'a mut Vfs<M>,
    managed: EntryMap,
    paths: PathMap,
    goal_revision: NonZeroUsize,
    view_changed: bool,
}

impl<M: PathAccessModel + Sized> Drop for RevisingVfs<'_, M> {
    fn drop(&mut self) {
        if self.view_changed {
            self.inner.managed = Arc::new(Mutex::new(std::mem::take(&mut self.managed)));
            self.inner.paths = Arc::new(Mutex::new(std::mem::take(&mut self.paths)));
            let revision = &mut self.inner.revision;
            *revision = self.goal_revision;
        }
    }
}

impl<M: PathAccessModel + Sized> RevisingVfs<'_, M> {
    /// Returns the underlying vfs.
    pub fn vfs(&mut self) -> &mut Vfs<M> {
        self.inner
    }

    fn am(&mut self) -> &mut VfsAccessModel<M> {
        &mut self.inner.access_model
    }

    fn invalidate_path(&mut self, path: &Path, snap: Option<&FileSnapshot>) {
        if let Some(fids) = self.paths.get(path) {
            if fids.is_empty() {
                return;
            }

            // Always changes view if snap is none.
            self.view_changed = snap.is_none();
            for fid in fids.clone() {
                self.invalidate_file_id(fid, snap);
            }
        }
    }

    fn invalidate_file_id(&mut self, file_id: FileId, snap: Option<&FileSnapshot>) {
        let mut changed = false;
        self.managed.slot(file_id, |e| {
            if let Some(snap) = snap {
                let may_read_bytes = e.bytes.get().map(|b| &b.2);
                match (snap, may_read_bytes) {
                    (FileSnapshot(Ok(snap)), Some(Ok(read))) if snap == read => {
                        return;
                    }
                    (FileSnapshot(Err(snap)), Some(Err(read))) if snap.as_ref() == read => {
                        return;
                    }
                    _ => {}
                }
            }

            e.changed_at = self.goal_revision.get();
            e.bytes = Arc::default();
            e.source = Arc::default();
            changed = true;
        });
        self.view_changed = changed;
    }

    /// Reset the shadowing files in [`OverlayAccessModel`].
    pub fn reset_shadow(&mut self) {
        for path in self.am().inner.inner.file_paths() {
            self.invalidate_path(&path, None);
        }
        for fid in self.am().file_paths() {
            self.invalidate_file_id(fid, None);
        }

        self.am().clear_shadow();
        self.am().inner.inner.clear_shadow();
    }

    /// Unconditionally changes the view of the vfs.
    pub fn change_view(&mut self) -> FileResult<()> {
        self.view_changed = true;
        Ok(())
    }

    /// Adds a shadowing file to the [`OverlayAccessModel`].
    pub fn map_shadow(&mut self, path: &Path, snap: FileSnapshot) -> FileResult<()> {
        self.invalidate_path(path, Some(&snap));
        self.am().inner.inner.add_file(path, snap, |c| c.into());

        Ok(())
    }

    /// Removes a shadowing file from the [`OverlayAccessModel`].
    pub fn unmap_shadow(&mut self, path: &Path) -> FileResult<()> {
        self.invalidate_path(path, None);
        self.am().inner.inner.remove_file(path);

        Ok(())
    }

    /// Adds a shadowing file to the [`OverlayAccessModel`] by file id.
    pub fn map_shadow_by_id(&mut self, file_id: FileId, snap: FileSnapshot) -> FileResult<()> {
        self.invalidate_file_id(file_id, Some(&snap));
        self.am().add_file(&file_id, snap, |c| *c);

        Ok(())
    }

    /// Removes a shadowing file from the [`OverlayAccessModel`] by file id.
    pub fn remove_shadow_by_id(&mut self, file_id: FileId) {
        self.invalidate_file_id(file_id, None);
        self.am().remove_file(&file_id);
    }

    /// Notifies the access model with a filesystem event.
    ///
    /// See [`NotifyAccessModel`] for more information.
    pub fn notify_fs_event(&mut self, event: FilesystemEvent) {
        self.notify_fs_changes(event.split().0);
    }
    /// Notifies the access model with a filesystem changes.
    ///
    /// See [`NotifyAccessModel`] for more information.
    pub fn notify_fs_changes(&mut self, event: FileChangeSet) {
        for path in &event.removes {
            self.invalidate_path(path, None);
        }
        for (path, snap) in &event.inserts {
            self.invalidate_path(path, Some(snap));
        }

        self.am().inner.inner.inner.notify(event);
    }
}

type BytesQuery = Arc<OnceLock<(Option<ImmutPath>, usize, FileResult<Bytes>)>>;

#[derive(Debug, Clone, Default)]
struct VfsEntry {
    changed_at: usize,
    bytes: BytesQuery,
    source: Arc<OnceLock<FileResult<Source>>>,
}

#[derive(Debug, Clone, Default)]
struct EntryMap {
    entries: RedBlackTreeMapSync<FileId, VfsEntry>,
}

impl EntryMap {
    /// Read a slot.
    #[inline(always)]
    fn slot<T>(&mut self, path: FileId, f: impl FnOnce(&mut VfsEntry) -> T) -> T {
        if let Some(entry) = self.entries.get_mut(&path) {
            f(entry)
        } else {
            let mut entry = VfsEntry::default();
            let res = f(&mut entry);
            self.entries.insert_mut(path, entry);
            res
        }
    }

    fn display(&self) -> DisplayEntryMap {
        DisplayEntryMap { map: self }
    }
}

/// A display wrapper for `EntryMap`.
pub struct DisplayEntryMap<'a> {
    map: &'a EntryMap,
}

impl fmt::Debug for DisplayEntryMap<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.map.entries.iter()).finish()
    }
}

#[derive(Debug, Clone, Default)]
struct PathMap {
    paths: FxHashMap<ImmutPath, EcoVec<FileId>>,
    file_ids: FxHashMap<FileId, (ImmutPath, NonZeroUsize)>,
}

impl PathMap {
    fn insert(&mut self, next: &ImmutPath, fid: FileId, rev: NonZeroUsize) {
        use std::collections::hash_map::Entry;
        let rev_entry = self.file_ids.entry(fid);

        match rev_entry {
            Entry::Occupied(mut entry) => {
                let (prev, prev_rev) = entry.get_mut();
                if prev != next {
                    if *prev_rev == rev {
                        log::warn!("Vfs: {fid:?} is changed in rev({rev:?}), {prev:?} -> {next:?}");
                    }

                    if let Some(fids) = self.paths.get_mut(prev) {
                        fids.retain(|f| *f != fid);
                    }

                    *prev = next.clone();
                    *prev_rev = rev;

                    self.paths.entry(next.clone()).or_default().push(fid);
                }
            }
            Entry::Vacant(entry) => {
                entry.insert((next.clone(), rev));
                self.paths.entry(next.clone()).or_default().push(fid);
            }
        }
    }

    fn get(&mut self, path: &Path) -> Option<&EcoVec<FileId>> {
        self.paths.get(path)
    }

    fn display(&self) -> DisplayPathMap {
        DisplayPathMap { map: self }
    }
}

/// A display wrapper for `PathMap`.
pub struct DisplayPathMap<'a> {
    map: &'a PathMap,
}

impl fmt::Debug for DisplayPathMap<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.map.paths.iter()).finish()
    }
}

/// Convert a byte slice to a string, removing UTF-8 BOM if present.
fn from_utf8_or_bom(buf: &[u8]) -> FileResult<&str> {
    Ok(std::str::from_utf8(if buf.starts_with(b"\xef\xbb\xbf") {
        // remove UTF-8 BOM
        &buf[3..]
    } else {
        // Assume UTF-8
        buf
    })?)
}

#[cfg(test)]
mod tests {
    fn is_send<T: Send>() {}
    fn is_sync<T: Sync>() {}

    #[test]
    fn test_vfs_send_sync() {
        is_send::<super::Vfs<super::dummy::DummyAccessModel>>();
        is_sync::<super::Vfs<super::dummy::DummyAccessModel>>();
    }
}
