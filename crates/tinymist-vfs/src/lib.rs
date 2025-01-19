//! upstream of following files <https://github.com/rust-lang/rust-analyzer/tree/master/crates/vfs>
//!   ::path_interner.rs -> path_interner.rs

#![allow(missing_docs)]

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

use rpds::RedBlackTreeMapSync;
pub use typst::foundations::Bytes;
pub use typst::syntax::FileId as TypstFileId;

pub use tinymist_std::time::Time;
pub use tinymist_std::ImmutPath;
use typst::syntax::Source;

use core::fmt;
use std::num::NonZeroUsize;
use std::sync::OnceLock;
use std::{path::Path, sync::Arc};

use parking_lot::{Mutex, RwLock};
use typst::diag::{FileError, FileResult};

use crate::notify::NotifyAccessModel;
use crate::overlay::OverlayAccessModel;
use crate::resolve::ResolveAccessModel;

/// Handle to a file in [`Vfs`]
pub type FileId = TypstFileId;

/// A trait for accessing underlying file system.
///
/// This trait is simplified by [`Vfs`] and requires a minimal method set for
/// typst compilation.
pub trait PathAccessModel {
    /// Clear the cache of the access model.
    ///
    /// This is called when the vfs is reset. See [`Vfs`]'s reset method for
    /// more information.
    fn reset(&mut self) {}

    /// Return the content of a file entry.
    fn content(&self, src: &Path) -> FileResult<Bytes>;
}

/// A trait for accessing underlying file system.
///
/// This trait is simplified by [`Vfs`] and requires a minimal method set for
/// typst compilation.
pub trait AccessModel {
    /// Clear the cache of the access model.
    ///
    /// This is called when the vfs is reset. See [`Vfs`]'s reset method for
    /// more information.
    fn reset(&mut self) {}

    /// Return the content of a file entry.
    fn content(&self, src: TypstFileId) -> (Option<ImmutPath>, FileResult<Bytes>);
}

#[derive(Clone)]
pub struct SharedAccessModel<M> {
    pub inner: Arc<RwLock<M>>,
}

impl<M> SharedAccessModel<M> {
    pub fn new(inner: M) -> Self {
        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }
}

impl<M> PathAccessModel for SharedAccessModel<M>
where
    M: PathAccessModel,
{
    #[inline]
    fn reset(&mut self) {
        self.inner.write().reset();
    }

    fn content(&self, src: &Path) -> FileResult<Bytes> {
        self.inner.read().content(src)
    }
}

type VfsPathAccessModel<M> = OverlayAccessModel<ImmutPath, NotifyAccessModel<SharedAccessModel<M>>>;
/// we add notify access model here since notify access model doesn't introduce
/// overheads by our observation
type VfsAccessModel<M> = OverlayAccessModel<TypstFileId, ResolveAccessModel<VfsPathAccessModel<M>>>;

pub trait FsProvider {
    /// Arbitrary one of file path corresponding to the given `id`.
    fn file_path(&self, id: TypstFileId) -> FileResult<PathResolution>;

    fn read(&self, id: TypstFileId) -> FileResult<Bytes>;
    fn read_source(&self, id: TypstFileId) -> FileResult<Source>;
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

#[derive(Default, Clone)]
pub struct SourceCache {
    /// The cache entries for each paths
    cache_entries: Arc<FxDashMap<TypstFileId, SourceIdShard>>,
}

impl SourceCache {
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

/// Create a new `Vfs` harnessing over the given `access_model` specific for
/// `reflexo_world::CompilerWorld`. With vfs, we can minimize the
/// implementation overhead for [`AccessModel`] trait.
pub struct Vfs<M: PathAccessModel + Sized> {
    source_cache: SourceCache,
    // The slots for all the files during a single lifecycle.
    // pub slots: Arc<Mutex<FxHashMap<TypstFileId, SourceCache>>>,
    managed: Arc<Mutex<EntryMap>>,
    paths: Arc<Mutex<PathMap>>,
    revision: NonZeroUsize,
    // access_model: TraceAccessModel<VfsAccessModel<M>>,
    /// The wrapped access model.
    access_model: VfsAccessModel<M>,
}

impl<M: PathAccessModel + Sized> fmt::Debug for Vfs<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vfs").finish()
    }
}

impl<M: PathAccessModel + Clone + Sized> Vfs<M> {
    pub fn revision(&self) -> NonZeroUsize {
        self.revision
    }

    pub fn snapshot(&self) -> Self {
        Self {
            source_cache: self.source_cache.clone(),
            managed: self.managed.clone(),
            paths: self.paths.clone(),
            revision: self.revision,
            access_model: self.access_model.clone(),
        }
    }

    pub fn take_state(&self) -> SourceCache {
        self.source_cache.clone()
    }
}

impl<M: PathAccessModel + Sized> Vfs<M> {
    /// Create a new `Vfs` with a given `access_model`.
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
        let access_model = SharedAccessModel::new(access_model);
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
            revision: NonZeroUsize::new(1).expect("initial revision is 1"),
            access_model,
        }
    }

    /// Reset all state.
    pub fn reset_all(&mut self) {
        self.reset_access_model();
        self.reset_cache();
    }

    /// Reset access model.
    pub fn reset_access_model(&mut self) {
        self.access_model.reset();
    }

    /// Reset all possible caches.
    pub fn reset_cache(&mut self) {
        self.revise().reset_cache();
    }

    /// Clear the cache that is not touched for a long time.
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

    /// Resolve the real path for a file id.
    pub fn file_path(&self, id: TypstFileId) -> Result<PathResolution, FileError> {
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
    pub fn shadow_ids(&self) -> Vec<TypstFileId> {
        self.access_model.file_paths()
    }

    /// Returns the overall memory usage for the stored files.
    pub fn memory_usage(&self) -> usize {
        0
    }

    pub fn revise(&mut self) -> RevisingVfs<M> {
        let managed = self.managed.lock().clone();
        let paths = self.paths.lock().clone();

        RevisingVfs {
            managed,
            paths,
            inner: self,
            view_changed: false,
        }
    }

    /// Reads a file.
    pub fn read(&self, fid: TypstFileId) -> FileResult<Bytes> {
        let bytes = self.managed.lock().slot(fid, |entry| entry.bytes.clone());

        self.read_content(&bytes, fid).clone()
    }

    /// Reads a source.
    pub fn source(&self, file_id: TypstFileId) -> FileResult<Source> {
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
    fn read_content<'a>(&self, bytes: &'a BytesQuery, fid: TypstFileId) -> &'a FileResult<Bytes> {
        &bytes
            .get_or_init(|| {
                let (path, content) = self.access_model.content(fid);
                if let Some(path) = path.as_ref() {
                    self.paths.lock().insert(path, fid);
                }

                (path, self.revision.get(), content)
            })
            .2
    }
}

pub struct RevisingVfs<'a, M: PathAccessModel + Sized> {
    inner: &'a mut Vfs<M>,
    managed: EntryMap,
    paths: PathMap,
    view_changed: bool,
}

impl<M: PathAccessModel + Sized> Drop for RevisingVfs<'_, M> {
    fn drop(&mut self) {
        if self.view_changed {
            self.inner.managed = Arc::new(Mutex::new(std::mem::take(&mut self.managed)));
            self.inner.paths = Arc::new(Mutex::new(std::mem::take(&mut self.paths)));
            let revision = &mut self.inner.revision;
            *revision = revision.checked_add(1).expect("revision overflowed");
        }
    }
}

impl<M: PathAccessModel + Sized> RevisingVfs<'_, M> {
    pub fn vfs(&mut self) -> &mut Vfs<M> {
        self.inner
    }

    fn am(&mut self) -> &mut VfsAccessModel<M> {
        &mut self.inner.access_model
    }

    fn invalidate_path(&mut self, path: &Path) {
        if let Some(fids) = self.paths.remove(path) {
            self.view_changed = true;
            for fid in fids {
                self.invalidate_file_id(fid);
            }
        }
    }

    fn invalidate_file_id(&mut self, file_id: TypstFileId) {
        self.view_changed = true;
        self.managed.slot(file_id, |e| {
            e.bytes = Arc::default();
            e.source = Arc::default();
        });
    }

    /// Reset the shadowing files in [`OverlayAccessModel`].
    ///
    /// Note: This function is independent from [`Vfs::reset`].
    pub fn reset_shadow(&mut self) {
        for path in self.am().inner.inner.file_paths() {
            self.invalidate_path(&path);
        }
        for fid in self.am().file_paths() {
            self.invalidate_file_id(fid);
        }

        self.am().clear_shadow();
        self.am().inner.inner.clear_shadow();
    }

    /// Reset all caches. This can happen when:
    /// - package paths are reconfigured.
    /// - The root of the workspace is switched.
    pub fn reset_cache(&mut self) {
        self.view_changed = true;
        self.managed = EntryMap::default();
        self.paths = PathMap::default();
    }

    /// Add a shadowing file to the [`OverlayAccessModel`].
    pub fn map_shadow(&mut self, path: &Path, snap: FileSnapshot) -> FileResult<()> {
        self.view_changed = true;
        self.invalidate_path(path);
        self.am().inner.inner.add_file(path, snap, |c| c.into());

        Ok(())
    }

    /// Remove a shadowing file from the [`OverlayAccessModel`].
    pub fn unmap_shadow(&mut self, path: &Path) -> FileResult<()> {
        self.view_changed = true;
        self.invalidate_path(path);
        self.am().inner.inner.remove_file(path);

        Ok(())
    }

    /// Add a shadowing file to the [`OverlayAccessModel`] by file id.
    pub fn map_shadow_by_id(&mut self, file_id: TypstFileId, snap: FileSnapshot) -> FileResult<()> {
        self.view_changed = true;
        self.invalidate_file_id(file_id);
        self.am().add_file(&file_id, snap, |c| *c);

        Ok(())
    }

    /// Remove a shadowing file from the [`OverlayAccessModel`] by file id.
    pub fn remove_shadow_by_id(&mut self, file_id: TypstFileId) {
        self.view_changed = true;
        self.invalidate_file_id(file_id);
        self.am().remove_file(&file_id);
    }

    /// Let the vfs notify the access model with a filesystem event.
    ///
    /// See [`NotifyAccessModel`] for more information.
    pub fn notify_fs_event(&mut self, event: FilesystemEvent) {
        self.notify_fs_changes(event.split().0);
    }
    /// Let the vfs notify the access model with a filesystem changes.
    ///
    /// See [`NotifyAccessModel`] for more information.
    pub fn notify_fs_changes(&mut self, event: FileChangeSet) {
        self.view_changed = true;
        self.am().inner.inner.inner.notify(event);
    }
}

type BytesQuery = Arc<OnceLock<(Option<ImmutPath>, usize, FileResult<Bytes>)>>;

#[derive(Debug, Clone, Default)]
struct VfsEntry {
    bytes: BytesQuery,
    source: Arc<OnceLock<FileResult<Source>>>,
}

#[derive(Clone, Default)]
struct EntryMap {
    entries: RedBlackTreeMapSync<TypstFileId, VfsEntry>,
}

impl EntryMap {
    /// Read a slot.
    #[inline(always)]
    fn slot<T>(&mut self, path: TypstFileId, f: impl FnOnce(&mut VfsEntry) -> T) -> T {
        if let Some(entry) = self.entries.get_mut(&path) {
            f(entry)
        } else {
            let mut entry = VfsEntry::default();
            let res = f(&mut entry);
            self.entries.insert_mut(path, entry);
            res
        }
    }
}

#[derive(Clone, Default)]
struct PathMap {
    paths: FxHashMap<ImmutPath, Vec<TypstFileId>>,
}

impl PathMap {
    fn insert(&mut self, path: &ImmutPath, fid: TypstFileId) {
        if let Some(fids) = self.paths.get_mut(path) {
            fids.push(fid);
        } else {
            self.paths.insert(path.clone(), vec![fid]);
        }
    }

    fn remove(&mut self, path: &Path) -> Option<Vec<TypstFileId>> {
        self.paths.remove(path)
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
