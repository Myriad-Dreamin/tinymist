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
/// Provides resolve access model.
pub mod resolve;
/// Provides trace access model which traces the underlying access model.
pub mod trace;
mod utils;

mod path_mapper;
use notify::{FilesystemEvent, NotifyAccessModel};
pub use path_mapper::{PathMapper, WorkspaceResolution, WorkspaceResolver};

use resolve::ResolveAccessModel;
pub use typst::foundations::Bytes;
pub use typst::syntax::FileId as TypstFileId;

pub use tinymist_std::time::Time;
pub use tinymist_std::ImmutPath;

use core::fmt;
use std::{hash::Hash, path::Path, sync::Arc};

use parking_lot::RwLock;
use typst::diag::{FileError, FileResult};

use self::overlay::OverlayAccessModel;

/// Handle to a file in [`Vfs`]
///
/// Most functions in typst-ts use this when they need to refer to a file.
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct FileId(pub TypstFileId);

/// safe because `FileId` is a new type of `u32`
impl nohash_hasher::IsEnabled for FileId {}

/// A trait for accessing underlying file system.
///
/// This trait is simplified by [`Vfs`] and requires a minimal method set for
/// typst compilation.
pub trait PathAccessModel {
    /// Clear the cache of the access model.
    ///
    /// This is called when the vfs is reset. See [`Vfs`]'s reset method for
    /// more information.
    fn clear(&mut self) {}

    /// Return whether a path is corresponding to a file.
    fn is_file(&self, src: &Path) -> FileResult<bool>;

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
    fn clear(&mut self) {}

    /// Return whether a path is corresponding to a file.
    fn is_file(&self, src: TypstFileId) -> FileResult<bool>;

    /// Return the content of a file entry.
    fn content(&self, src: TypstFileId) -> FileResult<Bytes>;
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
    fn clear(&mut self) {
        self.inner.write().clear();
    }

    fn is_file(&self, src: &Path) -> FileResult<bool> {
        self.inner.read().is_file(src)
    }

    fn content(&self, src: &Path) -> FileResult<Bytes> {
        self.inner.read().content(src)
    }
}

/// we add notify access model here since notify access model doesn't introduce
/// overheads by our observation
type VfsAccessModel<M> =
    ResolveAccessModel<OverlayAccessModel<NotifyAccessModel<SharedAccessModel<M>>>>;

pub trait FsProvider {
    /// Arbitrary one of file path corresponding to the given `id`.
    fn file_path(&self, id: TypstFileId) -> FileResult<ImmutPath>;

    fn read(&self, id: TypstFileId) -> FileResult<Bytes>;

    fn is_file(&self, id: TypstFileId) -> FileResult<bool>;
}
/// Create a new `Vfs` harnessing over the given `access_model` specific for
/// `reflexo_world::CompilerWorld`. With vfs, we can minimize the
/// implementation overhead for [`AccessModel`] trait.
pub struct Vfs<M: PathAccessModel + Sized> {
    // access_model: TraceAccessModel<VfsAccessModel<M>>,
    /// The wrapped access model.
    pub access_model: VfsAccessModel<M>,
}

impl<M: PathAccessModel + Sized> fmt::Debug for Vfs<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vfs").finish()
    }
}

impl<M: PathAccessModel + Clone + Sized> Vfs<M> {
    pub fn snapshot(&self) -> Self {
        Self {
            access_model: self.access_model.clone(),
        }
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
    pub fn new(resolver: Arc<dyn PathMapper + Send + Sync>, access_model: M) -> Self {
        let access_model = SharedAccessModel::new(access_model);
        let access_model = NotifyAccessModel::new(access_model);
        let access_model = OverlayAccessModel::new(access_model);
        let access_model = ResolveAccessModel {
            resolver,
            inner: access_model,
        };

        // If you want to trace the access model, uncomment the following line
        // let access_model = TraceAccessModel::new(access_model);

        Self { access_model }
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
        self.access_model.inner.clear_shadow();
    }

    /// Get paths to all the shadowing files in [`OverlayAccessModel`].
    pub fn shadow_paths(&self) -> Vec<ImmutPath> {
        self.access_model.inner.file_paths()
    }

    /// Add a shadowing file to the [`OverlayAccessModel`].
    pub fn map_shadow(&mut self, path: &Path, content: Bytes) -> FileResult<()> {
        self.access_model.inner.add_file(path, content);

        Ok(())
    }

    /// Remove a shadowing file from the [`OverlayAccessModel`].
    pub fn remove_shadow(&mut self, path: &Path) {
        self.access_model.inner.remove_file(path);
    }

    /// Let the vfs notify the access model with a filesystem event.
    ///
    /// See [`NotifyAccessModel`] for more information.
    pub fn notify_fs_event(&mut self, event: FilesystemEvent) {
        self.access_model.inner.inner.notify(event);
    }

    /// Returns the overall memory usage for the stored files.
    pub fn memory_usage(&self) -> usize {
        0
    }

    /// Read a file.
    pub fn read(&self, path: TypstFileId) -> FileResult<Bytes> {
        if self.access_model.is_file(path)? {
            self.access_model.content(path)
        } else {
            Err(FileError::IsDirectory)
        }
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
