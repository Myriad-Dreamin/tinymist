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
/// Provides notify access model which retrieves file system events and changes
/// from some notify backend.
pub mod notify;
/// Provides overlay access model which allows to shadow the underlying access
/// model with memory contents.
pub mod overlay;
/// Provides trace access model which traces the underlying access model.
pub mod trace;
mod utils;

mod path_interner;

pub use typst::foundations::Bytes;
pub use typst::syntax::FileId as TypstFileId;

pub use tinymist_std::time::Time;
pub use tinymist_std::ImmutPath;

pub(crate) use path_interner::PathInterner;

use core::fmt;
use std::{collections::HashMap, hash::Hash, path::Path, sync::Arc};

use parking_lot::{Mutex, RwLock};
use tinymist_std::path::PathClean;
use typst::diag::{FileError, FileResult};

use self::{
    notify::{FilesystemEvent, NotifyAccessModel},
    overlay::OverlayAccessModel,
};

/// Handle to a file in [`Vfs`]
///
/// Most functions in typst-ts use this when they need to refer to a file.
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct FileId(pub u32);

/// safe because `FileId` is a new type of `u32`
impl nohash_hasher::IsEnabled for FileId {}

/// A trait for accessing underlying file system.
///
/// This trait is simplified by [`Vfs`] and requires a minimal method set for
/// typst compilation.
pub trait AccessModel {
    /// Clear the cache of the access model.
    ///
    /// This is called when the vfs is reset. See [`Vfs`]'s reset method for
    /// more information.
    fn clear(&mut self) {}
    /// Return a mtime corresponding to the path.
    ///
    /// Note: vfs won't touch the file entry if mtime is same between vfs reset
    /// lifecycles for performance design.
    fn mtime(&self, src: &Path) -> FileResult<Time>;

    /// Return whether a path is corresponding to a file.
    fn is_file(&self, src: &Path) -> FileResult<bool>;

    /// Return the real path before creating a vfs file entry.
    ///
    /// Note: vfs will fetch the file entry once if multiple paths shares a same
    /// real path.
    fn real_path(&self, src: &Path) -> FileResult<ImmutPath>;

    /// Return the content of a file entry.
    fn content(&self, src: &Path) -> FileResult<Bytes>;
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

impl<M> AccessModel for SharedAccessModel<M>
where
    M: AccessModel,
{
    fn clear(&mut self) {
        self.inner.write().clear();
    }

    fn mtime(&self, src: &Path) -> FileResult<Time> {
        self.inner.read().mtime(src)
    }

    fn is_file(&self, src: &Path) -> FileResult<bool> {
        self.inner.read().is_file(src)
    }

    fn real_path(&self, src: &Path) -> FileResult<ImmutPath> {
        self.inner.read().real_path(src)
    }

    fn content(&self, src: &Path) -> FileResult<Bytes> {
        self.inner.read().content(src)
    }
}

/// we add notify access model here since notify access model doesn't introduce
/// overheads by our observation
type VfsAccessModel<M> = OverlayAccessModel<NotifyAccessModel<SharedAccessModel<M>>>;

pub trait FsProvider {
    /// Arbitrary one of file path corresponding to the given `id`.
    fn file_path(&self, id: FileId) -> ImmutPath;

    fn mtime(&self, id: FileId) -> FileResult<Time>;

    fn read(&self, id: FileId) -> FileResult<Bytes>;

    fn is_file(&self, id: FileId) -> FileResult<bool>;
}

#[derive(Default)]
struct PathMapper {
    /// Map from path to slot index.
    ///
    /// Note: we use a owned [`FileId`] here, which is resultant from
    /// [`PathInterner`]
    id_cache: RwLock<HashMap<ImmutPath, FileId>>,
    /// The path interner for canonical paths.
    intern: Mutex<PathInterner<ImmutPath, ()>>,
}

impl PathMapper {
    /// Id of the given path if it exists in the `Vfs` and is not deleted.
    pub fn file_id(&self, path: &Path) -> FileId {
        let quick_id = self.id_cache.read().get(path).copied();
        if let Some(id) = quick_id {
            return id;
        }

        let path: ImmutPath = path.clean().as_path().into();

        let mut path_interner = self.intern.lock();
        let id = path_interner.intern(path.clone(), ()).0;

        let mut path2slot = self.id_cache.write();
        path2slot.insert(path.clone(), id);

        id
    }

    /// File path corresponding to the given `file_id`.
    pub fn file_path(&self, file_id: FileId) -> ImmutPath {
        let path_interner = self.intern.lock();
        path_interner.lookup(file_id).clone()
    }
}

/// Create a new `Vfs` harnessing over the given `access_model` specific for
/// `reflexo_world::CompilerWorld`. With vfs, we can minimize the
/// implementation overhead for [`AccessModel`] trait.
pub struct Vfs<M: AccessModel + Sized> {
    paths: Arc<PathMapper>,

    // access_model: TraceAccessModel<VfsAccessModel<M>>,
    /// The wrapped access model.
    access_model: VfsAccessModel<M>,
}

impl<M: AccessModel + Sized> fmt::Debug for Vfs<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vfs").finish()
    }
}

impl<M: AccessModel + Clone + Sized> Vfs<M> {
    pub fn snapshot(&self) -> Self {
        Self {
            paths: self.paths.clone(),
            access_model: self.access_model.clone(),
        }
    }
}

impl<M: AccessModel + Sized> Vfs<M> {
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
    pub fn new(access_model: M) -> Self {
        let access_model = SharedAccessModel::new(access_model);
        let access_model = NotifyAccessModel::new(access_model);
        let access_model = OverlayAccessModel::new(access_model);

        // If you want to trace the access model, uncomment the following line
        // let access_model = TraceAccessModel::new(access_model);

        Self {
            paths: Default::default(),
            access_model,
        }
    }

    /// Reset the source file and path references.
    ///
    /// It performs a rolling reset, with discard some cache file entry when it
    /// is unused in recent 30 lifecycles.
    ///
    /// Note: The lifetime counter is incremented every time this function is
    /// called.
    pub fn reset(&mut self) {
        self.access_model.clear();
    }

    /// Reset the shadowing files in [`OverlayAccessModel`].
    ///
    /// Note: This function is independent from [`Vfs::reset`].
    pub fn reset_shadow(&mut self) {
        self.access_model.clear_shadow();
    }

    /// Get paths to all the shadowing files in [`OverlayAccessModel`].
    pub fn shadow_paths(&self) -> Vec<Arc<Path>> {
        self.access_model.file_paths()
    }

    /// Add a shadowing file to the [`OverlayAccessModel`].
    pub fn map_shadow(&mut self, path: &Path, content: Bytes) -> FileResult<()> {
        self.access_model.add_file(path.into(), content);

        Ok(())
    }

    /// Remove a shadowing file from the [`OverlayAccessModel`].
    pub fn remove_shadow(&mut self, path: &Path) {
        self.access_model.remove_file(path);
    }

    /// Let the vfs notify the access model with a filesystem event.
    ///
    /// See [`NotifyAccessModel`] for more information.
    pub fn notify_fs_event(&mut self, event: FilesystemEvent) {
        self.access_model.inner.notify(event);
    }

    /// Returns the overall memory usage for the stored files.
    pub fn memory_usage(&self) -> usize {
        0
    }

    /// Id of the given path if it exists in the `Vfs` and is not deleted.
    pub fn file_id(&self, path: &Path) -> FileId {
        self.paths.file_id(path)
    }

    /// Read a file.
    pub fn read(&self, path: &Path) -> FileResult<Bytes> {
        if self.access_model.is_file(path)? {
            self.access_model.content(path)
        } else {
            Err(FileError::IsDirectory)
        }
    }
}

impl<M: AccessModel> FsProvider for Vfs<M> {
    fn file_path(&self, id: FileId) -> ImmutPath {
        self.paths.file_path(id)
    }

    fn mtime(&self, src: FileId) -> FileResult<Time> {
        self.access_model.mtime(&self.file_path(src))
    }

    fn read(&self, src: FileId) -> FileResult<Bytes> {
        self.access_model.content(&self.file_path(src))
    }

    fn is_file(&self, src: FileId) -> FileResult<bool> {
        self.access_model.is_file(&self.file_path(src))
    }
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
