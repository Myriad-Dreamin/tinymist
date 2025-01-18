// use std::sync::Arc;

use core::fmt;
use std::{num::NonZeroUsize, sync::Arc};

use parking_lot::{Mutex, RwLock};
use tinymist_std::hash::FxHashMap;
use tinymist_std::{ImmutPath, QueryRef};
use tinymist_vfs::{Bytes, FileId, FsProvider, TypstFileId};
use typst::{
    diag::{FileError, FileResult},
    syntax::Source,
};

/// incrementally query a value from a self holding state
type IncrQueryRef<S, E> = QueryRef<S, E, Option<S>>;

type FileQuery<T> = QueryRef<T, FileError>;
type IncrFileQuery<T> = IncrQueryRef<T, FileError>;

pub trait Revised {
    fn last_accessed_rev(&self) -> NonZeroUsize;
}

pub struct SharedState<T> {
    pub committed_revision: Option<usize>,
    // todo: fine-grained lock
    /// The cache entries for each paths
    cache_entries: FxHashMap<TypstFileId, T>,
}

impl<T> fmt::Debug for SharedState<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SharedState")
            .field("committed_revision", &self.committed_revision)
            .finish()
    }
}

impl<T> Default for SharedState<T> {
    fn default() -> Self {
        SharedState {
            committed_revision: None,
            cache_entries: FxHashMap::default(),
        }
    }
}

impl<T: Revised> SharedState<T> {
    fn gc(&mut self) {
        let committed = self.committed_revision.unwrap_or(0);
        self.cache_entries
            .retain(|_, v| committed.saturating_sub(v.last_accessed_rev().get()) <= 30);
    }
}

pub struct SourceCache {
    last_accessed_rev: NonZeroUsize,
    fid: FileId,
    source: IncrFileQuery<Source>,
    buffer: FileQuery<Bytes>,
}

impl fmt::Debug for SourceCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceCache").finish()
    }
}

impl Revised for SourceCache {
    fn last_accessed_rev(&self) -> NonZeroUsize {
        self.last_accessed_rev
    }
}

pub struct SourceState {
    pub revision: NonZeroUsize,
    pub slots: Arc<Mutex<FxHashMap<TypstFileId, SourceCache>>>,
}

impl SourceState {
    pub fn commit_impl(self, state: &mut SharedState<SourceCache>) {
        log::debug!("drop source db revision {}", self.revision);

        if let Ok(slots) = Arc::try_unwrap(self.slots) {
            // todo: utilize the committed revision is not zero
            if state
                .committed_revision
                .is_some_and(|committed| committed >= self.revision.get())
            {
                return;
            }

            log::debug!("committing source db revision {}", self.revision);
            state.committed_revision = Some(self.revision.get());
            state.cache_entries = slots.into_inner();
            state.gc();
        }
    }
}

#[derive(Clone)]
pub struct SourceDb {
    pub revision: NonZeroUsize,
    pub shared: Arc<RwLock<SharedState<SourceCache>>>,
    /// The slots for all the files during a single lifecycle.
    pub slots: Arc<Mutex<FxHashMap<TypstFileId, SourceCache>>>,
}

impl fmt::Debug for SourceDb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceDb").finish()
    }
}

impl SourceDb {
    pub fn take_state(&mut self) -> SourceState {
        SourceState {
            revision: self.revision,
            slots: std::mem::take(&mut self.slots),
        }
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
    pub fn iter_dependencies_dyn<'a>(
        &'a self,
        p: &'a impl FsProvider,
        f: &mut dyn FnMut(ImmutPath),
    ) {
        for slot in self.slots.lock().iter() {
            f(p.file_path(slot.1.fid));
        }
    }

    /// Get file content by path.
    pub fn file(&self, id: TypstFileId, fid: FileId, p: &impl FsProvider) -> FileResult<Bytes> {
        self.slot(id, fid, |slot| slot.buffer.compute(|| p.read(fid)).cloned())
    }

    /// Get source content by path and assign the source with a given typst
    /// global file id.
    ///
    /// See `Vfs::resolve_with_f` for more information.
    pub fn source(&self, id: TypstFileId, fid: FileId, p: &impl FsProvider) -> FileResult<Source> {
        self.slot(id, fid, |slot| {
            slot.source
                .compute_with_context(|prev| {
                    let content = p.read(fid)?;
                    let next = from_utf8_or_bom(&content)?.to_owned();

                    // otherwise reparse the source
                    match prev {
                        Some(mut source) => {
                            source.replace(&next);
                            Ok(source)
                        }
                        // Return a new source if we don't have a reparse feature or no prev
                        _ => Ok(Source::new(id, next)),
                    }
                })
                .cloned()
        })
    }

    /// Insert a new slot into the vfs.
    fn slot<T>(&self, id: TypstFileId, fid: FileId, f: impl FnOnce(&SourceCache) -> T) -> T {
        let mut slots = self.slots.lock();
        f(slots.entry(id).or_insert_with(|| {
            let state = self.shared.read();
            let cache_entry = state.cache_entries.get(&id);

            cache_entry
                .map(|e| SourceCache {
                    last_accessed_rev: self.revision.max(e.last_accessed_rev),
                    fid,
                    source: IncrFileQuery::with_context(
                        e.source
                            .get_uninitialized()
                            .cloned()
                            .transpose()
                            .ok()
                            .flatten(),
                    ),
                    buffer: FileQuery::default(),
                })
                .unwrap_or_else(|| SourceCache {
                    last_accessed_rev: self.revision,
                    fid,
                    source: IncrFileQuery::with_context(None),
                    buffer: FileQuery::default(),
                })
        }))
    }
}

pub trait MergeCache: Sized {
    fn merge(self, _other: Self) -> Self {
        self
    }
}

pub struct FontDb {}
pub struct PackageDb {}

/// Convert a byte slice to a string, removing UTF-8 BOM if present.
fn from_utf8_or_bom(buf: &[u8]) -> FileResult<&str> {
    Ok(std::str::from_utf8(if buf.starts_with(b"\xef\xbb\xbf") {
        // remove UTF-8 BOM
        &buf[3..]
    } else {
        // Assume UTF-8
        buf
    })?)
}
