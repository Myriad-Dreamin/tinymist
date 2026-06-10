use std::{borrow::Borrow, hash::Hash, path::Path};

use tinymist_std::{ImmutPath, hash::FxHashMap};
use typst::diag::FileResult;

use crate::{AccessModel, Bytes, FileId, FileSnapshot, PathAccessModel};

/// Provides overlay access model which allows to shadow the underlying access
/// model with memory contents.
#[derive(Default, Debug, Clone)]
pub struct OverlayAccessModel<K: Eq + Hash, M> {
    files: FxHashMap<K, FileSnapshot>,
    /// The underlying access model
    pub inner: M,
}

impl<K: Eq + Hash + Clone, M> OverlayAccessModel<K, M> {
    /// Create a new [`OverlayAccessModel`] with the given inner access model
    pub fn new(inner: M) -> Self {
        Self {
            files: FxHashMap::default(),
            inner,
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
        self.files = FxHashMap::default();
    }

    /// Get the shadowed file paths
    pub fn file_paths(&self) -> Vec<K> {
        self.files.keys().cloned().collect()
    }

    /// Add a shadow file to the [`OverlayAccessModel`]
    pub fn add_file<Q: Eq + Hash + ?Sized>(
        &mut self,
        path: &Q,
        snap: FileSnapshot,
        cast: impl Fn(&Q) -> K,
    ) where
        K: Borrow<Q>,
    {
        self.files.insert(cast(path), snap);
    }

    /// Remove a shadow file from the [`OverlayAccessModel`]
    pub fn remove_file<Q: Eq + Hash + ?Sized>(&mut self, path: &Q)
    where
        K: Borrow<Q>,
    {
        self.files.remove(path);
    }
}

impl<M: PathAccessModel> PathAccessModel for OverlayAccessModel<ImmutPath, M> {
    fn content(&self, src: &Path) -> FileResult<Bytes> {
        if let Some(content) = self.files.get(src) {
            return content.content().cloned();
        }

        self.inner.content(src)
    }
}

impl<M: AccessModel> AccessModel for OverlayAccessModel<FileId, M> {
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn content(&self, src: FileId) -> (Option<ImmutPath>, FileResult<Bytes>) {
        if let Some(content) = self.files.get(&src) {
            return (None, content.content().cloned());
        }

        self.inner.content(src)
    }
}
