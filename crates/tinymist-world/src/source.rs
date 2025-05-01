// use std::sync::Arc;

use core::fmt;
use std::sync::Arc;

use parking_lot::Mutex;
use tinymist_std::{hash::FxHashMap, QueryRef};
use tinymist_vfs::{Bytes, FileId, FsProvider};
use typst::diag::{FileError, FileResult};
use typst::syntax::Source;

type FileQuery<T> = QueryRef<T, FileError>;

pub struct SourceCache {
    touched_by_compile: bool,
    fid: FileId,
    source: FileQuery<Source>,
    buffer: FileQuery<Bytes>,
}

impl fmt::Debug for SourceCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceCache").finish()
    }
}

#[derive(Clone)]
pub struct SourceDb {
    pub is_compiling: bool,
    /// The slots for all the files during a single lifecycle.
    pub slots: Arc<Mutex<FxHashMap<FileId, SourceCache>>>,
}

impl fmt::Debug for SourceDb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceDb").finish()
    }
}

impl SourceDb {
    pub fn set_is_compiling(&mut self, is_compiling: bool) {
        self.is_compiling = is_compiling;
    }

    /// Returns the overall memory usage for the stored files.
    pub fn memory_usage(&self) -> usize {
        let mut w = self.slots.lock().len() * core::mem::size_of::<SourceCache>();
        w += self
            .slots
            .lock()
            .iter()
            .map(|(_, slot)| {
                slot.source
                    .get_uninitialized()
                    .and_then(|e| e.as_ref().ok())
                    .map_or(16, |e| e.text().len() * 8)
                    + slot
                        .buffer
                        .get_uninitialized()
                        .and_then(|e| e.as_ref().ok())
                        .map_or(16, |e| e.len())
            })
            .sum::<usize>();

        w
    }

    /// Get all the files that are currently in the VFS.
    ///
    /// This is typically corresponds to the file dependencies of a single
    /// compilation.
    ///
    /// When you don't reset the vfs for each compilation, this function will
    /// still return remaining files from the previous compilation.
    pub fn iter_dependencies_dyn(&self, f: &mut dyn FnMut(FileId)) {
        for slot in self.slots.lock().values() {
            if !slot.touched_by_compile {
                continue;
            }
            f(slot.fid);
        }
    }

    /// Get file content by path.
    pub fn file(&self, fid: FileId, p: &impl FsProvider) -> FileResult<Bytes> {
        self.slot(fid, |slot| slot.buffer.compute(|| p.read(fid)).cloned())
    }

    /// Get source content by path and assign the source with a given typst
    /// global file id.
    ///
    /// See `Vfs::resolve_with_f` for more information.
    pub fn source(&self, fid: FileId, p: &impl FsProvider) -> FileResult<Source> {
        self.slot(fid, |slot| {
            slot.source.compute(|| p.read_source(fid)).cloned()
        })
    }

    /// Insert a new slot into the vfs.
    fn slot<T>(&self, fid: FileId, f: impl FnOnce(&SourceCache) -> T) -> T {
        let mut slots = self.slots.lock();
        f({
            let entry = slots.entry(fid).or_insert_with(|| SourceCache {
                touched_by_compile: self.is_compiling,
                fid,
                source: FileQuery::default(),
                buffer: FileQuery::default(),
            });
            if self.is_compiling && !entry.touched_by_compile {
                // We put the mutation behind the if statement to avoid
                // unnecessary writes to the cache.
                entry.touched_by_compile = true;
            }
            entry
        })
    }

    pub(crate) fn take_state(&mut self) -> Self {
        let slots = std::mem::take(&mut self.slots);

        Self {
            is_compiling: self.is_compiling,
            slots,
        }
    }
}
