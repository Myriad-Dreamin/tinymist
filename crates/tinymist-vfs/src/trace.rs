use std::sync::atomic::AtomicU64;

use tinymist_std::ImmutPath;
use typst::diag::FileResult;

use crate::{AccessModel, Bytes, FileId};

/// Provides trace access model which traces the underlying access model.
///
/// It simply wraps the underlying access model and prints all the access to the
/// stdout or the browser console.
#[derive(Debug)]
pub struct TraceAccessModel<M: AccessModel + Sized> {
    trace: [AtomicU64; 6],
    /// The inner access model
    pub inner: M,
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
    #[inline]
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn content(&self, src: FileId) -> (Option<ImmutPath>, FileResult<Bytes>) {
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
