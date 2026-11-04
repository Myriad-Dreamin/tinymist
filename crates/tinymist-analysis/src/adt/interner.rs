//! Global `Arc`-based object interning infrastructure.
//!
//! Eventually this should probably be replaced with salsa-based interning.
//!
//! todo: This is less efficient as the arc object will change its reference
//! count every time it is cloned. todo: we may be able to optimize use by
//! following approach:
//! ```plain
//! fn run_analyze(f) {
//!   let local = thread_local_intern();
//!   let res = f(local);
//!   std::thread::spawn(move || gc(local));
//!   return res
//! }
//! ```
//! However, this is out of scope for now.

use std::{
    fmt::{self, Debug, Display},
    hash::{BuildHasherDefault, Hash, Hasher},
    mem::ManuallyDrop,
    ops::Deref,
    sync::{LazyLock, OnceLock},
};

use dashmap::{DashMap, SharedValue};
use ecow::{EcoString, EcoVec};
use hashbrown::{HashMap, hash_map::RawEntryMut};
use parking_lot::Mutex;
use rustc_hash::FxHasher;
use triomphe::Arc;
use typst::{foundations::Str, syntax::ast::Ident};

type InternMap<T> = DashMap<Arc<T>, (), BuildHasherDefault<FxHasher>>;
type Guard<T> = dashmap::RwLockWriteGuard<
    'static,
    HashMap<Arc<T>, SharedValue<()>, BuildHasherDefault<FxHasher>>,
>;

// https://news.ycombinator.com/item?id=22220342

pub struct Interned<T: Internable + ?Sized> {
    arc: ManuallyDrop<Arc<T>>,
}

impl<T: Internable + ?Sized> Interned<T> {
    #[inline]
    fn arc(&self) -> &Arc<T> {
        &self.arc
    }
}

impl<T: Internable> Interned<T> {
    pub fn new(obj: T) -> Self {
        let (mut shard, hash) = Self::select(&obj);
        // Atomically,
        // - check if `obj` is already in the map
        //   - if so, clone its `Arc` and return it
        //   - if not, box it up, insert it, and return a clone
        // This needs to be atomic (locking the shard) to avoid races with other thread,
        // which could insert the same object between us looking it up and
        // inserting it.
        match shard.raw_entry_mut().from_key_hashed_nocheck(hash, &obj) {
            RawEntryMut::Occupied(occ) => Self {
                arc: ManuallyDrop::new(occ.key().clone()),
            },
            RawEntryMut::Vacant(vac) => {
                T::storage().alloc().increment();
                Self {
                    arc: ManuallyDrop::new(
                        vac.insert_hashed_nocheck(hash, Arc::new(obj), SharedValue::new(()))
                            .0
                            .clone(),
                    ),
                }
            }
        }
    }
}

// Note: It is dangerous to keep interned object temporarily (u128)
// Case:
// ```
// insert(hash(Interned::new_str("a"))) == true
// insert(hash(Interned::new_str("a"))) == true
// ```
impl Interned<str> {
    pub fn new_str(s: &str) -> Self {
        let (mut shard, hash) = Self::select(s);
        // Atomically,
        // - check if `obj` is already in the map
        //   - if so, clone its `Arc` and return it
        //   - if not, box it up, insert it, and return a clone
        // This needs to be atomic (locking the shard) to avoid races with other thread,
        // which could insert the same object between us looking it up and
        // inserting it.
        match shard.raw_entry_mut().from_key_hashed_nocheck(hash, s) {
            RawEntryMut::Occupied(occ) => Self {
                arc: ManuallyDrop::new(occ.key().clone()),
            },
            RawEntryMut::Vacant(vac) => {
                str::storage().alloc().increment();

                Self {
                    arc: ManuallyDrop::new(
                        vac.insert_hashed_nocheck(hash, Arc::from(s), SharedValue::new(()))
                            .0
                            .clone(),
                    ),
                }
            }
        }
    }
}

static EMPTY: LazyLock<Interned<str>> = LazyLock::new(|| Interned::new_str(""));
impl Default for Interned<str> {
    fn default() -> Self {
        EMPTY.clone()
    }
}

impl Interned<str> {
    pub fn empty() -> &'static Self {
        &EMPTY
    }
}

impl From<&str> for Interned<str> {
    fn from(s: &str) -> Self {
        Interned::new_str(s)
    }
}

impl From<Str> for Interned<str> {
    fn from(s: Str) -> Self {
        Interned::new_str(&s)
    }
}

impl From<EcoString> for Interned<str> {
    fn from(s: EcoString) -> Self {
        Interned::new_str(&s)
    }
}

impl From<&EcoString> for Interned<str> {
    fn from(s: &EcoString) -> Self {
        Interned::new_str(s)
    }
}

impl From<Ident<'_>> for Interned<str> {
    fn from(s: Ident<'_>) -> Self {
        Interned::new_str(s.get())
    }
}

impl From<&Interned<str>> for EcoString {
    fn from(s: &Interned<str>) -> Self {
        s.as_ref().into()
    }
}

impl<T: Internable> From<T> for Interned<T> {
    fn from(s: T) -> Self {
        Interned::new(s)
    }
}

impl<T: Internable + Clone> From<&T> for Interned<T> {
    fn from(s: &T) -> Self {
        Interned::new(s.clone())
    }
}

impl<T: Internable + ?Sized> Interned<T> {
    #[inline]
    fn hash(obj: &T) -> u64 {
        let storage = T::storage().get();
        let mut hasher = std::hash::BuildHasher::build_hasher(storage.hasher());
        obj.hash(&mut hasher);
        hasher.finish()
    }

    #[inline]
    fn select_hashed(hash: u64) -> Guard<T> {
        let storage = T::storage().get();
        let shard_idx = storage.determine_shard(hash as usize);
        storage.shards()[shard_idx].write()
    }

    #[inline]
    fn select(obj: &T) -> (Guard<T>, u64) {
        let hash = Self::hash(obj);
        (Self::select_hashed(hash), hash)
    }
}

impl<T: Internable + ?Sized> Drop for Interned<T> {
    fn drop(&mut self) {
        // SAFETY: Drop runs once for this value. This takes its sole field owner
        // before any fallible work; the local Arc is dropped on every exit and
        // unwind path. The field is never read or dropped again, and self is
        // not moved after taking it. ManuallyDrop preserves Arc's null niche.
        let arc = unsafe { ManuallyDrop::take(&mut self.arc) };
        let storage = T::storage();
        let release = storage.release.lock();

        // Serialize the count check and decrement. Without this, two final
        // callers can both observe three owners and leave only the map alive.
        if Arc::count(&arc) != 2 {
            // Another caller and the map still own the value, so this cannot
            // run T's destructor while the release lock is held.
            drop(arc);
            return;
        }

        // Hash may itself release interned handles. Preserve its original
        // behavior by running it outside both interner locks.
        drop(release);
        let hash = Self::hash(&arc);
        let mut shard = Self::select_hashed(hash);
        let release = storage.release.lock();

        // Interning may have added an owner while we computed the hash or
        // waited for the shard. Release under the same protocol in that case.
        if Arc::count(&arc) != 2 {
            drop(shard);
            drop(arc);
            return;
        }

        // Removal needs this canonical allocation, not value equality. Avoid
        // invoking user equality callbacks while the release lock is held.
        match shard
            .raw_entry_mut()
            .from_hash(hash, |key| Arc::ptr_eq(key, &arc))
        {
            RawEntryMut::Occupied(occ) => {
                occ.remove();
            }
            RawEntryMut::Vacant(_) => unreachable!(),
        }
        storage.alloc().decrement();
        // Shrinking may rehash other keys; keep its original shard-only
        // callback environment, permitting nonfinal handle releases.
        drop(release);
        if shard.len() * 2 < shard.capacity() {
            shard.shrink_to_fit();
        }

        // T may recursively release values from this type and shard.
        drop(shard);
        drop(arc);
    }
}

/// Compares interned `Ref`s using pointer equality.
impl<T: Internable> PartialEq for Interned<T> {
    // NOTE: No `?Sized` because `ptr_eq` doesn't work right with trait objects.

    #[inline]
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(self.arc(), other.arc())
    }
}

impl<T: Internable> Eq for Interned<T> {}

impl<T: Internable + PartialOrd> PartialOrd for Interned<T> {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self == other {
            Some(std::cmp::Ordering::Equal)
        } else {
            self.as_ref().partial_cmp(other.as_ref())
        }
    }
}

impl<T: Internable + Ord> Ord for Interned<T> {
    #[inline]
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self == other {
            std::cmp::Ordering::Equal
        } else {
            self.as_ref().cmp(other.as_ref())
        }
    }
}

impl PartialOrd for Interned<str> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Interned<str> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self == other {
            std::cmp::Ordering::Equal
        } else {
            self.as_ref().cmp(other.as_ref())
        }
    }
}

impl PartialEq for Interned<str> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(self.arc(), other.arc())
    }
}

impl Eq for Interned<str> {}

impl serde::Serialize for Interned<str> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.arc().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Interned<str> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StrVisitor;

        impl serde::de::Visitor<'_> for StrVisitor {
            type Value = Interned<str>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(Interned::new_str(v))
            }
        }

        deserializer.deserialize_str(StrVisitor)
    }
}

impl<T: Internable + ?Sized> Hash for Interned<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // NOTE: Cast disposes vtable pointer / slice/str length.
        state.write_usize(Arc::as_ptr(self.arc()) as *const () as usize)
    }
}

impl<T: Internable + ?Sized> AsRef<T> for Interned<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        self.arc()
    }
}

impl<T: Internable + ?Sized> Deref for Interned<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.arc()
    }
}

impl<T: Internable + ?Sized> Clone for Interned<T> {
    fn clone(&self) -> Self {
        Self {
            arc: ManuallyDrop::new(self.arc().clone()),
        }
    }
}

impl<T: Debug + Internable + ?Sized> Debug for Interned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (*self.arc()).fmt(f)
    }
}

impl<T: Display + Internable + ?Sized> Display for Interned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (*self.arc()).fmt(f)
    }
}

pub static MAPS: Mutex<EcoVec<(&'static str, usize, Arc<AllocStats>)>> = Mutex::new(EcoVec::new());

pub struct InternStorage<T: ?Sized> {
    // Protect the caller count check together with its Arc decrement.
    release: Mutex<()>,
    alloc: OnceLock<Arc<AllocStats>>,
    map: OnceLock<InternMap<T>>,
}

#[allow(clippy::new_without_default)] // this a const fn, so it can't be default
impl<T: InternSize + ?Sized> InternStorage<T> {
    const SIZE: usize = T::INTERN_SIZE;

    pub const fn new() -> Self {
        Self {
            release: Mutex::new(()),
            alloc: OnceLock::new(),
            map: OnceLock::new(),
        }
    }
}

impl<T: Internable + ?Sized> InternStorage<T> {
    fn alloc(&self) -> &Arc<AllocStats> {
        self.alloc.get_or_init(Arc::default)
    }

    fn get(&self) -> &InternMap<T> {
        self.map.get_or_init(|| {
            MAPS.lock()
                .push((std::any::type_name::<T>(), Self::SIZE, self.alloc().clone()));
            DashMap::default()
        })
    }
}

pub trait InternSize {
    const INTERN_SIZE: usize;
}

impl<T: Sized> InternSize for T {
    const INTERN_SIZE: usize = std::mem::size_of::<T>();
}

impl InternSize for str {
    const INTERN_SIZE: usize = std::mem::size_of::<usize>() * 2;
}

pub trait Internable: InternSize + Hash + Eq + 'static {
    fn storage() -> &'static InternStorage<Self>;
}

/// Implements `Internable` for a given list of types, making them usable with
/// `Interned`.
#[macro_export]
#[doc(hidden)]
macro_rules! _impl_internable {
    ( $($t:ty),+ $(,)? ) => { $(
        impl $crate::adt::interner::Internable for $t {
            fn storage() -> &'static $crate::adt::interner::InternStorage<Self> {
                static STORAGE: $crate::adt::interner::InternStorage<$t> = $crate::adt::interner::InternStorage::new();
                &STORAGE
            }
        }
    )+ };
}

pub use crate::_impl_internable as impl_internable;
use crate::stats::AllocStats;

impl_internable!(str,);

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    use super::*;

    static PARTIAL_CMP_CALLS: AtomicUsize = AtomicUsize::new(0);

    #[derive(Eq, Hash, PartialEq)]
    struct Counted(u8);

    impl PartialOrd for Counted {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            PARTIAL_CMP_CALLS.fetch_add(1, AtomicOrdering::Relaxed);
            self.0.partial_cmp(&other.0)
        }
    }

    impl_internable!(Counted);

    #[test]
    fn partial_cmp_short_circuits_shared_values() {
        let value = Interned::new(Counted(1));

        PARTIAL_CMP_CALLS.store(0, AtomicOrdering::Relaxed);
        assert_eq!(value.partial_cmp(&value.clone()), Some(Ordering::Equal));
        assert_eq!(PARTIAL_CMP_CALLS.load(AtomicOrdering::Relaxed), 0);

        let other = Interned::new(Counted(2));
        assert_eq!(value.partial_cmp(&other), Some(Ordering::Less));
        assert_eq!(PARTIAL_CMP_CALLS.load(AtomicOrdering::Relaxed), 1);
    }
}

#[cfg(test)]
mod regression {
    use super::*;
    pub(super) fn counts<T: Internable>() -> (usize, usize, usize) {
        let st = T::storage();
        let a = st.alloc();
        use std::sync::atomic::Ordering;
        (
            a.allocated.load(Ordering::Relaxed),
            a.dropped.load(Ordering::Relaxed),
            st.get().len(),
        )
    }
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    #[derive(Hash, Eq, PartialEq)]
    struct RaceKey(u64);
    impl_internable!(RaceKey);
    #[test]
    fn concurrent_drops_and_reinterning_release_every_value() {
        let gate = Arc::new(Barrier::new(3));
        for round in 0..2000 {
            let a = Interned::new(RaceKey(round));
            let b = a.clone();
            let g1 = gate.clone();
            let g2 = gate.clone();
            std::thread::scope(|s| {
                s.spawn(move || {
                    g1.wait();
                    drop(a);
                });
                s.spawn(move || {
                    g2.wait();
                    drop(b);
                });
                gate.wait();
                drop(Interned::new(RaceKey(round)));
            });
        }
        assert_eq!(counts::<RaceKey>().2, 0);
        let (a, d, _) = counts::<RaceKey>();
        assert_eq!(a, d);
    }
    static DROPS: AtomicUsize = AtomicUsize::new(0);
    #[derive(Eq, PartialEq)]
    struct Nested {
        key: u64,
        child: Option<Interned<Nested>>,
    }
    impl std::hash::Hash for Nested {
        fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
            h.write_u8(0);
        }
    }
    impl Drop for Nested {
        fn drop(&mut self) {
            DROPS.fetch_add(1, Ordering::Relaxed);
        }
    }
    impl_internable!(Nested);
    #[test]
    fn nested_same_type_destructors_do_not_deadlock() {
        let mut value = None;
        for key in 0..100 {
            value = Some(Interned::new(Nested { key, child: value }));
        }
        drop(value);
        assert_eq!(DROPS.load(Ordering::Relaxed), 100);
        assert_eq!(counts::<Nested>(), (100, 100, 0));
    }
    #[test]
    fn strings_clone_and_release() {
        let a = Interned::new_str("synthetic");
        let b = a.clone();
        assert_eq!(a, b);
        drop(a);
        assert_eq!(&*b, "synthetic");
        drop(b);
    }
    #[test]
    fn representation_stays_one_pointer() {
        assert_eq!(
            std::mem::size_of::<Interned<RaceKey>>(),
            std::mem::size_of::<usize>()
        );
        assert_eq!(
            std::mem::size_of::<Interned<str>>(),
            std::mem::size_of::<triomphe::Arc<str>>()
        );
        assert_eq!(
            std::mem::size_of::<Option<Interned<RaceKey>>>(),
            std::mem::size_of::<Option<triomphe::Arc<RaceKey>>>()
        );
        assert_eq!(
            std::mem::size_of::<Option<Interned<str>>>(),
            std::mem::size_of::<Option<triomphe::Arc<str>>>()
        );
    }
}

#[cfg(test)]
mod stress {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    #[derive(Hash, Eq, PartialEq)]
    struct Key(u64);
    static DESTRUCTORS: AtomicUsize = AtomicUsize::new(0);
    impl Drop for Key {
        fn drop(&mut self) {
            DESTRUCTORS.fetch_add(1, Ordering::Relaxed);
        }
    }
    impl_internable!(Key);
    #[test]
    fn eight_callers_release_unique_payloads() {
        let gate = Arc::new(Barrier::new(8));
        let batches: Vec<Vec<_>> = (0..8).map(|_| Vec::new()).collect();
        let mut batches = batches;
        for key in 0..10000 {
            let v = Interned::new(Key(key));
            for b in &mut batches {
                b.push(v.clone());
            }
        }
        std::thread::scope(|scope| {
            for batch in batches {
                let g = gate.clone();
                scope.spawn(move || {
                    g.wait();
                    for item in batch {
                        drop(item);
                    }
                });
            }
        });
        assert_eq!(super::regression::counts::<Key>(), (10000, 10000, 0));
        assert_eq!(DESTRUCTORS.load(Ordering::Relaxed), 10000);
    }
}

#[cfg(test)]
mod callback_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    static SIDE: Mutex<Option<Interned<HashDropKey>>> = Mutex::new(None);
    static RELEASE_SIDE: AtomicBool = AtomicBool::new(false);
    #[derive(Eq, PartialEq)]
    struct HashDropKey(u64);
    impl Hash for HashDropKey {
        fn hash<H: Hasher>(&self, h: &mut H) {
            h.write_u8(0);
            if self.0 == 0 && RELEASE_SIDE.swap(false, Ordering::SeqCst) {
                let side = SIDE.lock().take();
                drop(side);
            }
        }
    }
    impl_internable!(HashDropKey);
    #[test]
    fn shrinking_hash_callback_can_release_nonfinal_handle() {
        let anchor = Interned::new(HashDropKey(0));
        *SIDE.lock() = Some(anchor.clone());
        let others: Vec<_> = (1..100).map(|i| Interned::new(HashDropKey(i))).collect();
        RELEASE_SIDE.store(true, Ordering::SeqCst);
        drop(others);
        assert!(!RELEASE_SIDE.load(Ordering::SeqCst));
        assert!(SIDE.lock().is_none());
        drop(anchor);
        assert_eq!(super::regression::counts::<HashDropKey>(), (100, 100, 0));
    }
}

#[cfg(test)]
mod collision_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static EQ_CALLS: AtomicUsize = AtomicUsize::new(0);
    struct CollisionKey(u64);
    impl Hash for CollisionKey {
        fn hash<H: Hasher>(&self, h: &mut H) {
            h.write_u8(0);
        }
    }
    impl PartialEq for CollisionKey {
        fn eq(&self, other: &Self) -> bool {
            EQ_CALLS.fetch_add(1, Ordering::Relaxed);
            self.0 == other.0
        }
    }
    impl Eq for CollisionKey {}
    impl_internable!(CollisionKey);
    #[test]
    fn removal_uses_identity_without_equality_callbacks() {
        let values: Vec<_> = (0..100).map(|i| Interned::new(CollisionKey(i))).collect();
        let before = EQ_CALLS.load(Ordering::Relaxed);
        assert!(before > 0);
        drop(values);
        assert_eq!(EQ_CALLS.load(Ordering::Relaxed), before);
        assert_eq!(super::regression::counts::<CollisionKey>(), (100, 100, 0));
    }
}
