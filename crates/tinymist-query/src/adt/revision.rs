use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, OnceLock},
};

pub struct RevisionLock {
    estimated: usize,
    used: OnceLock<usize>,
}

impl RevisionLock {
    pub fn access(&self, revision: NonZeroUsize) {
        self.used
            .set(revision.get())
            .unwrap_or_else(|_| panic!("revision {revision} is determined"))
    }
}

pub struct RevisionSlot<T> {
    pub revision: usize,
    pub data: T,
}

impl<T> std::ops::Deref for RevisionSlot<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> std::ops::DerefMut for RevisionSlot<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

pub struct RevisionManager<T> {
    estimated: usize,
    locked: HashMap<usize, usize>,
    slots: Vec<Arc<RevisionSlot<T>>>,
}

impl<T> Default for RevisionManager<T> {
    fn default() -> Self {
        Self {
            estimated: 0,
            locked: Default::default(),
            slots: Default::default(),
        }
    }
}

impl<T> RevisionManager<T> {
    pub fn clear(&mut self) {
        self.slots.clear();
    }

    /// Lock the revision in *main thread*.
    #[must_use]
    pub fn lock(&mut self, used: NonZeroUsize) -> RevisionLock {
        let l = self.lock_estimated();
        l.access(used);
        l
    }

    /// Lock the revision in *main thread*.
    #[must_use]
    pub fn lock_estimated(&mut self) -> RevisionLock {
        let estimated = self.estimated;
        *self.locked.entry(estimated).or_default() += 1;
        RevisionLock {
            estimated,
            used: OnceLock::new(),
        }
    }

    /// Find the last revision slot by revision number.
    pub fn find_revision(
        &mut self,
        revision: NonZeroUsize,
        f: impl FnOnce(Option<&Arc<RevisionSlot<T>>>) -> T,
    ) -> Arc<RevisionSlot<T>> {
        let slot_base = self
            .slots
            .iter()
            .filter(|e| e.revision <= revision.get())
            .reduce(|a, b| if a.revision > b.revision { a } else { b });

        if let Some(slot) = slot_base {
            if slot.revision == revision.get() {
                return slot.clone();
            }
        }

        let slot = Arc::new(RevisionSlot {
            revision: revision.get(),
            data: f(slot_base),
        });
        self.slots.push(slot.clone());
        self.estimated = revision.get().max(self.estimated);
        slot
    }

    pub fn unlock(&mut self, rev: &mut RevisionLock) -> Option<usize> {
        let rev = rev.estimated;
        let revision_cnt = self
            .locked
            .entry(rev)
            .or_insert_with(|| panic!("revision {rev} is not locked"));
        *revision_cnt -= 1;
        if *revision_cnt != 0 {
            return None;
        }

        self.locked.remove(&rev);
        let existing = self.locked.keys().min().copied();
        existing.or_else(||
            // if there is no locked revision, we only keep the latest revision
            self.slots
                .iter()
                .map(|e| e.revision)
                .max())
    }
}

pub trait RevisionManagerLike {
    fn gc(&mut self, min_rev: usize);
}

impl<T> RevisionManagerLike for RevisionManager<T> {
    fn gc(&mut self, min_rev: usize) {
        self.slots.retain(|r| r.revision >= min_rev);
    }
}
