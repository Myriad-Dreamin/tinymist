use std::{borrow::Borrow, marker::PhantomData, num::NonZeroU16, path::Path};

use rpds::RedBlackTreeMapSync;
use tinymist_std::ImmutPath;
use typst::diag::FileResult;

use crate::{AccessModel, Bytes, FileId, FileSnapshot, PathAccessModel};

pub(crate) type RawFileId = NonZeroU16;

/// Provides overlay access model which allows to shadow the underlying access
/// model with memory contents.
#[derive(Default, Debug, Clone)]
pub struct OverlayAccessModel<K, M, S = K>
where
    S: Ord,
{
    files: RedBlackTreeMapSync<S, FileSnapshot>,
    /// The underlying access model
    pub inner: M,
    _key: PhantomData<fn() -> K>,
}

impl<K, M, S> OverlayAccessModel<K, M, S>
where
    S: Ord,
{
    /// Create a new [`OverlayAccessModel`] with the given inner access model
    pub fn new(inner: M) -> Self {
        Self {
            files: RedBlackTreeMapSync::default(),
            inner,
            _key: PhantomData,
        }
    }

    /// Get the inner access model
    pub fn inner(&self) -> &M {
        &self.inner
    }

    /// Get the mutable reference to the inner access model
    pub fn inner_mut(&mut self) -> &mut M {
        &mut self.inner
    }

    /// Clear the shadowed files
    pub fn clear_shadow(&mut self) {
        self.files = RedBlackTreeMapSync::default();
    }
}

impl<M> OverlayAccessModel<ImmutPath, M> {
    /// Get the shadowed file paths
    pub fn file_paths(&self) -> Vec<ImmutPath> {
        self.files.keys().cloned().collect()
    }

    /// Add a shadow file to the [`OverlayAccessModel`]
    pub fn add_file<Q: Ord + ?Sized>(
        &mut self,
        path: &Q,
        snap: FileSnapshot,
        cast: impl Fn(&Q) -> ImmutPath,
    ) where
        ImmutPath: Borrow<Q>,
    {
        match self.files.get_mut(path) {
            Some(e) => {
                *e = snap;
            }
            None => {
                self.files.insert_mut(cast(path), snap);
            }
        }
    }

    /// Remove a shadow file from the [`OverlayAccessModel`]
    pub fn remove_file<Q: Ord + ?Sized>(&mut self, path: &Q)
    where
        ImmutPath: Borrow<Q>,
    {
        self.files.remove_mut(path);
    }
}

impl<M> OverlayAccessModel<FileId, M, RawFileId> {
    /// Get the shadowed file ids
    pub fn file_paths(&self) -> Vec<FileId> {
        self.files.keys().copied().map(FileId::from_raw).collect()
    }

    /// Add a shadow file to the [`OverlayAccessModel`]
    pub fn add_file(&mut self, id: &FileId, snap: FileSnapshot, cast: impl Fn(&FileId) -> FileId) {
        match self.files.get_mut(&id.into_raw()) {
            Some(e) => {
                *e = snap;
            }
            None => {
                self.files.insert_mut(cast(id).into_raw(), snap);
            }
        }
    }

    /// Remove a shadow file from the [`OverlayAccessModel`]
    pub fn remove_file(&mut self, id: &FileId) {
        self.files.remove_mut(&id.into_raw());
    }
}

impl<M: PathAccessModel> PathAccessModel for OverlayAccessModel<ImmutPath, M> {
    fn is_system(&self) -> bool {
        self.inner.is_system()
    }

    fn content(&self, src: &Path) -> FileResult<Bytes> {
        if let Some(content) = self.files.get(src) {
            return content.content().cloned();
        }

        self.inner.content(src)
    }
}

impl<M: AccessModel> AccessModel for OverlayAccessModel<FileId, M, RawFileId> {
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn is_system(&self) -> bool {
        self.inner.is_system()
    }

    fn content(&self, src: FileId) -> (Option<ImmutPath>, FileResult<Bytes>) {
        if let Some(content) = self.files.get(&src.into_raw()) {
            return (None, content.content().cloned());
        }

        self.inner.content(src)
    }
}
