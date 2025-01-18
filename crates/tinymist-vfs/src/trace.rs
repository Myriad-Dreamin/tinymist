use std::{path::Path, sync::atomic::AtomicU64};

use typst::diag::FileResult;

use crate::{AccessModel, Bytes};

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

    fn mtime(&self, src: &Path) -> FileResult<crate::Time> {
        let instant = tinymist_std::time::Instant::now();
        let res = self.inner.mtime(src);
        let elapsed = instant.elapsed();
        // self.trace[0] += elapsed.as_nanos() as u64;
        self.trace[0].fetch_add(
            elapsed.as_nanos() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        crate::utils::console_log!("mtime: {:?} {:?} => {:?}", src, elapsed, res);
        res
    }

    fn is_file(&self, src: &Path) -> FileResult<bool> {
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

    fn content(&self, src: &Path) -> FileResult<Bytes> {
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
