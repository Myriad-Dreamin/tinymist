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
    ops::Deref,
    sync::OnceLock,
};

use dashmap::{DashMap, SharedValue};
use ecow::EcoString;
use fxhash::FxHasher;
use hashbrown::{hash_map::RawEntryMut, HashMap};
use triomphe::Arc;
use typst::{foundations::Str, syntax::ast::Ident};

type InternMap<T> = DashMap<Arc<T>, (), BuildHasherDefault<FxHasher>>;
type Guard<T> = dashmap::RwLockWriteGuard<
    'static,
    HashMap<Arc<T>, SharedValue<()>, BuildHasherDefault<FxHasher>>,
>;

pub struct Interned<T: Internable + ?Sized> {
    arc: Arc<T>,
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
                arc: occ.key().clone(),
            },
            RawEntryMut::Vacant(vac) => Self {
                arc: vac
                    .insert_hashed_nocheck(hash, Arc::new(obj), SharedValue::new(()))
                    .0
                    .clone(),
            },
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
                arc: occ.key().clone(),
            },
            RawEntryMut::Vacant(vac) => Self {
                arc: vac
                    .insert_hashed_nocheck(hash, Arc::from(s), SharedValue::new(()))
                    .0
                    .clone(),
            },
        }
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
    fn select(obj: &T) -> (Guard<T>, u64) {
        let storage = T::storage().get();
        let hash = {
            let mut hasher = std::hash::BuildHasher::build_hasher(storage.hasher());
            obj.hash(&mut hasher);
            hasher.finish()
        };
        let shard_idx = storage.determine_shard(hash as usize);
        let shard = &storage.shards()[shard_idx];
        (shard.write(), hash)
    }
}

impl<T: Internable + ?Sized> Drop for Interned<T> {
    #[inline]
    fn drop(&mut self) {
        // When the last `Ref` is dropped, remove the object from the global map.
        if Arc::count(&self.arc) == 2 {
            // Only `self` and the global map point to the object.

            self.drop_slow();
        }
    }
}

impl<T: Internable + ?Sized> Interned<T> {
    #[cold]
    fn drop_slow(&mut self) {
        let (mut shard, hash) = Self::select(&self.arc);

        if Arc::count(&self.arc) != 2 {
            // Another thread has interned another copy
            return;
        }

        match shard
            .raw_entry_mut()
            .from_key_hashed_nocheck(hash, &self.arc)
        {
            RawEntryMut::Occupied(occ) => occ.remove(),
            RawEntryMut::Vacant(_) => unreachable!(),
        };

        // Shrink the backing storage if the shard is less than 50% occupied.
        if shard.len() * 2 < shard.capacity() {
            shard.shrink_to_fit();
        }
    }
}

/// Compares interned `Ref`s using pointer equality.
impl<T: Internable> PartialEq for Interned<T> {
    // NOTE: No `?Sized` because `ptr_eq` doesn't work right with trait objects.

    #[inline]
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.arc, &other.arc)
    }
}

impl<T: Internable> Eq for Interned<T> {}

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
        Arc::ptr_eq(&self.arc, &other.arc)
    }
}

impl Eq for Interned<str> {}

impl<T: Internable + ?Sized> Hash for Interned<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // NOTE: Cast disposes vtable pointer / slice/str length.
        state.write_usize(Arc::as_ptr(&self.arc) as *const () as usize)
    }
}

impl<T: Internable + ?Sized> AsRef<T> for Interned<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        &self.arc
    }
}

impl<T: Internable + ?Sized> Deref for Interned<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.arc
    }
}

impl<T: Internable + ?Sized> Clone for Interned<T> {
    fn clone(&self) -> Self {
        Self {
            arc: self.arc.clone(),
        }
    }
}

impl<T: Debug + Internable + ?Sized> Debug for Interned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (*self.arc).fmt(f)
    }
}

impl<T: Display + Internable + ?Sized> Display for Interned<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (*self.arc).fmt(f)
    }
}

pub struct InternStorage<T: ?Sized> {
    map: OnceLock<InternMap<T>>,
}

#[allow(clippy::new_without_default)] // this a const fn, so it can't be default
impl<T: ?Sized> InternStorage<T> {
    pub const fn new() -> Self {
        Self {
            map: OnceLock::new(),
        }
    }
}

impl<T: Internable + ?Sized> InternStorage<T> {
    fn get(&self) -> &InternMap<T> {
        self.map.get_or_init(DashMap::default)
    }
}

pub trait Internable: Hash + Eq + 'static {
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

impl_internable!(str,);
