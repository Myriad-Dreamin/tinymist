use std::{borrow::Borrow, cmp::Ord, path::Path};

use rpds::RedBlackTreeMapSync;
use tinymist_std::ImmutPath;
use typst::diag::FileResult;

use crate::{AccessModel, Bytes, PathAccessModel, TypstFileId};

/// Provides overlay access model which allows to shadow the underlying access
/// model with memory contents.
#[derive(Default, Debug, Clone)]
pub struct OverlayAccessModel<K: Ord, M> {
    files: RedBlackTreeMapSync<K, Bytes>,
    /// The underlying access model
    pub inner: M,
}

impl<K: Ord + Clone, M> OverlayAccessModel<K, M> {
    /// Create a new [`OverlayAccessModel`] with the given inner access model
    pub fn new(inner: M) -> Self {
        Self {
            files: RedBlackTreeMapSync::default(),
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
        self.files = RedBlackTreeMapSync::default();
    }

    /// Get the shadowed file paths
    pub fn file_paths(&self) -> Vec<K> {
        self.files.keys().cloned().collect()
    }

    /// Add a shadow file to the [`OverlayAccessModel`]
    pub fn add_file<Q: Ord + ?Sized>(&mut self, path: &Q, content: Bytes, cast: impl Fn(&Q) -> K)
    where
        K: Borrow<Q>,
    {
        match self.files.get_mut(path) {
            Some(e) => {
                *e = content;
            }
            None => {
                self.files.insert_mut(cast(path), content);
            }
        }
    }

    /// Remove a shadow file from the [`OverlayAccessModel`]
    pub fn remove_file<Q: Ord + ?Sized>(&mut self, path: &Q)
    where
        K: Borrow<Q>,
    {
        self.files.remove_mut(path);
    }
}

impl<M: PathAccessModel> PathAccessModel for OverlayAccessModel<ImmutPath, M> {
    fn content(&self, src: &Path) -> FileResult<Bytes> {
        if let Some(content) = self.files.get(src) {
            return Ok(content.clone());
        }

        self.inner.content(src)
    }
}

impl<M: AccessModel> AccessModel for OverlayAccessModel<TypstFileId, M> {
    fn content(&self, src: TypstFileId) -> FileResult<Bytes> {
        if let Some(content) = self.files.get(&src) {
            return Ok(content.clone());
        }

        self.inner.content(src)
    }
}
