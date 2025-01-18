//! Maps paths to compact integer ids. We don't care about clearings paths which
//! no longer exist -- the assumption is total size of paths we ever look at is
//! not too big.
use std::hash::{BuildHasherDefault, Hash};

use indexmap::IndexMap;
use reflexo::hash::FxHasher;

use super::FileId;

/// Structure to map between [`VfsPath`] and [`FileId`].
#[derive(Debug)]
pub(crate) struct PathInterner<P, Ext = ()> {
    map: IndexMap<P, Ext, BuildHasherDefault<FxHasher>>,
}

impl<P, Ext> Default for PathInterner<P, Ext> {
    fn default() -> Self {
        Self {
            map: IndexMap::default(),
        }
    }
}

impl<P: Hash + Eq, Ext> PathInterner<P, Ext> {
    /// Scan through each value in the set and keep those where the
    /// closure `keep` returns `true`.
    ///
    /// The elements are visited in order, and remaining elements keep their
    /// order.
    ///
    /// Computes in **O(n)** time (average).
    #[allow(dead_code)]
    pub fn retain(&mut self, keep: impl FnMut(&P, &mut Ext) -> bool) {
        self.map.retain(keep)
    }

    /// Insert `path` in `self`.
    ///
    /// - If `path` already exists in `self`, returns its associated id;
    /// - Else, returns a newly allocated id.
    #[inline]
    pub(crate) fn intern(&mut self, path: P, ext: Ext) -> (FileId, Option<&mut Ext>) {
        let (id, _) = self.map.insert_full(path, ext);
        assert!(id < u32::MAX as usize);
        (FileId(id as u32), None)
    }

    /// Returns the path corresponding to `id`.
    ///
    /// # Panics
    ///
    /// Panics if `id` does not exists in `self`.
    #[allow(dead_code)]
    pub(crate) fn lookup(&self, id: FileId) -> &P {
        self.map.get_index(id.0 as usize).unwrap().0
    }
}

#[cfg(test)]
mod tests {
    use super::PathInterner;
    use std::path::PathBuf;

    #[test]
    fn test_interner_path_buf() {
        let mut interner = PathInterner::<PathBuf>::default();
        let (id, ..) = interner.intern(PathBuf::from("foo"), ());
        assert_eq!(interner.lookup(id), &PathBuf::from("foo"));
    }
}
