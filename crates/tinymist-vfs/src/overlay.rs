use std::path::Path;
use std::sync::Arc;

use rpds::RedBlackTreeMapSync;
use typst::diag::FileResult;

use crate::{AccessModel, Bytes, Time};

#[derive(Debug, Clone)]
struct OverlayFileMeta {
    mt: Time,
    content: Bytes,
}

/// Provides overlay access model which allows to shadow the underlying access
/// model with memory contents.
#[derive(Default, Debug, Clone)]
pub struct OverlayAccessModel<M> {
    files: RedBlackTreeMapSync<Arc<Path>, OverlayFileMeta>,
    /// The underlying access model
    pub inner: M,
}

impl<M: AccessModel> OverlayAccessModel<M> {
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
    pub fn file_paths(&self) -> Vec<Arc<Path>> {
        self.files.keys().cloned().collect()
    }

    /// Add a shadow file to the [`OverlayAccessModel`]
    pub fn add_file(&mut self, path: Arc<Path>, content: Bytes) {
        // we change mt every time, since content almost changes every time
        // Note: we can still benefit from cache, since we incrementally parse source

        let mt = tinymist_std::time::now();
        let meta = OverlayFileMeta { mt, content };

        match self.files.get_mut(&path) {
            Some(e) => {
                if e.mt == meta.mt && e.content != meta.content {
                    e.mt = meta
                        .mt
                        // [`crate::Time`] has a minimum resolution of 1ms
                        // we negate the time by 1ms so that the time is always
                        // invalidated
                        .checked_sub(std::time::Duration::from_millis(1))
                        .unwrap();
                    e.content = meta.content.clone();
                } else {
                    *e = meta.clone();
                }
            }
            None => {
                self.files.insert_mut(path, meta);
            }
        }
    }

    /// Remove a shadow file from the [`OverlayAccessModel`]
    pub fn remove_file(&mut self, path: &Path) {
        self.files.remove_mut(path);
    }
}

impl<M: AccessModel> AccessModel for OverlayAccessModel<M> {
    fn is_file(&self, src: &Path) -> FileResult<bool> {
        if self.files.get(src).is_some() {
            return Ok(true);
        }

        self.inner.is_file(src)
    }

    fn content(&self, src: &Path) -> FileResult<Bytes> {
        if let Some(meta) = self.files.get(src) {
            return Ok(meta.content.clone());
        }

        self.inner.content(src)
    }
}
