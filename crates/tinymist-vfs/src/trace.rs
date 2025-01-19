use std::sync::atomic::AtomicU64;

use typst::diag::FileResult;

use crate::{AccessModel, Bytes, TypstFileId};

/// Provides trace access model which traces the underlying access model.
///
/// It simply wraps the underlying access model and prints all the access to the
/// stdout or the browser console.
#[derive(Debug)]
pub struct TraceAccessModel<M: AccessModel + Sized> {
    pub inner: M,
    trace: [AtomicU64; 6],
}

impl<M: AccessModel + Sized> TraceAccessModel<M> {
    /// Create a new [`TraceAccessModel`] with the given inner access model
    pub fn new(inner: M) -> Self {
        Self {
            inner,
            trace: Default::default(),
        }
    }
}

impl<M: AccessModel + Sized> AccessModel for TraceAccessModel<M> {
    fn clear(&mut self) {
        self.inner.clear();
    }

    fn is_file(&self, src: TypstFileId) -> FileResult<bool> {
        let instant = tinymist_std::time::Instant::now();
        let res = self.inner.is_file(src);
        let elapsed = instant.elapsed();
        self.trace[1].fetch_add(
            elapsed.as_nanos() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        crate::utils::console_log!("is_file: {:?} {:?}", src, elapsed);
        res
    }

    fn content(&self, src: TypstFileId) -> FileResult<Bytes> {
        let instant = tinymist_std::time::Instant::now();
        let res = self.inner.content(src);
        let elapsed = instant.elapsed();
        self.trace[3].fetch_add(
            elapsed.as_nanos() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        crate::utils::console_log!("read_all: {:?} {:?}", src, elapsed);
        res
    }
}
