//! The hash extension module. It provides extra concepts like `Fingerprint` and
//! `HashedTrait`.

use core::fmt;
use std::any::Any;
use std::hash::{Hash, Hasher};
use std::ops::Deref;

use base64::Engine;
use fxhash::FxHasher32;
use siphasher::sip128::{Hasher128, SipHasher13};

#[cfg(feature = "rkyv")]
use rkyv::{Archive, Deserialize as rDeser, Serialize as rSer};

use crate::error::prelude::Result;

pub(crate) type FxBuildHasher = std::hash::BuildHasherDefault<FxHasher>;
pub use rustc_hash::{FxHashMap, FxHashSet, FxHasher};
// pub type FxIndexSet<K> = indexmap::IndexSet<K, FxHasher>;
// pub type FxIndexMap<K, V> = indexmap::IndexMap<K, V, FxHasher>;
/// A dashmap that uses the FxHasher as the underlying hasher.
pub type FxDashMap<K, V> = dashmap::DashMap<K, V, FxBuildHasher>;

/// See <https://github.com/rust-lang/rust/blob/master/compiler/rustc_hir/src/stable_hash_impls.rs#L22>
/// The fingerprint conflicts should be very rare and should be handled by the
/// compiler.
///
/// > That being said, given a high quality hash function, the collision
/// > probabilities in question are very small. For example, for a big crate
/// > like `rustc_middle` (with ~50000 `LocalDefId`s as of the time of writing)
/// > there is a probability of roughly 1 in 14,750,000,000 of a crate-internal
/// > collision occurring. For a big crate graph with 1000 crates in it, there
/// > is a probability of 1 in 36,890,000,000,000 of a `StableCrateId`
/// > collision.
#[derive(Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct Fingerprint {
    lo: u64,
    hi: u64,
}

impl fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_svg_id("fg"))
    }
}

impl serde::Serialize for Fingerprint {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_svg_id(""))
    }
}

impl<'de> serde::Deserialize<'de> for Fingerprint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = <std::string::String as serde::Deserialize>::deserialize(deserializer)?;
        Fingerprint::try_from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl Fingerprint {
    /// Create a new fingerprint from the given pair of 64-bit integers.
    pub fn from_pair(lo: u64, hi: u64) -> Self {
        Self { lo, hi }
    }

    /// Create a new fingerprint from the given 128-bit integer.
    pub const fn from_u128(hash: u128) -> Self {
        // Self(hash as u64, (hash >> 64) as u64)
        Self {
            lo: hash as u64,
            hi: (hash >> 64) as u64,
        }
    }

    /// Get the fingerprint as a 128-bit integer.
    pub fn to_u128(self) -> u128 {
        ((self.hi as u128) << 64) | self.lo as u128
    }

    /// Cut the fingerprint into a 32-bit integer.
    /// It could be used as a hash value if the fingerprint is calculated from a
    /// stable hash function.
    pub fn lower32(self) -> u32 {
        self.lo as u32
    }

    /// Creates a new `Fingerprint` from a svg id that **doesn't have prefix**.
    pub fn try_from_str(s: &str) -> Result<Self> {
        let bytes = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(&s.as_bytes()[..11])
            .expect("invalid base64 string");
        let lo = u64::from_le_bytes(bytes.try_into().unwrap());
        let mut bytes = base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(&s.as_bytes()[11..])
            .expect("invalid base64 string");
        bytes.resize(8, 0);
        let hi = u64::from_le_bytes(bytes.try_into().unwrap());
        Ok(Self::from_pair(lo, hi))
    }

    /// Create a xml id from the given prefix and the fingerprint of this
    /// reference. Note that the entire html document shares namespace for
    /// ids.
    #[comemo::memoize]
    pub fn as_svg_id(self, prefix: &'static str) -> String {
        let fingerprint_lo =
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(self.lo.to_le_bytes());
        if self.hi == 0 {
            return [prefix, &fingerprint_lo].join("");
        }

        // possible the id in the lower 64 bits.
        let fingerprint_hi = {
            let id = self.hi.to_le_bytes();
            // truncate zero
            let rev_zero = id.iter().rev().skip_while(|&&b| b == 0).count();
            let id = &id[..rev_zero];
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(id)
        };
        [prefix, &fingerprint_lo, &fingerprint_hi].join("")
    }
}

/// A fingerprint hasher that extends the [`std::hash::Hasher`] trait.
pub trait FingerprintHasher: std::hash::Hasher {
    /// Finish the fingerprint and return the fingerprint and the data.
    /// The data is used to resolve the conflict.
    fn finish_fingerprint(self) -> (Fingerprint, Vec<u8>);
}

/// A fingerprint hasher that uses the [`SipHasher13`] algorithm.
#[derive(Default)]
pub struct FingerprintSipHasher {
    /// The underlying data passed to the hasher.
    data: Vec<u8>,
}

/// The base hasher for the [`FingerprintSipHasher`].
pub type FingerprintSipHasherBase = SipHasher13;

impl FingerprintSipHasher {
    /// Get the fast hash value and the underlying data.
    pub fn fast_hash(&self) -> (u32, &Vec<u8>) {
        let mut inner = FxHasher32::default();
        self.data.hash(&mut inner);
        (inner.finish() as u32, &self.data)
    }
}

impl std::hash::Hasher for FingerprintSipHasher {
    fn write(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    fn finish(&self) -> u64 {
        let mut inner = FingerprintSipHasherBase::default();
        self.data.hash(&mut inner);
        inner.finish()
    }
}

impl FingerprintHasher for FingerprintSipHasher {
    fn finish_fingerprint(self) -> (Fingerprint, Vec<u8>) {
        let buffer = self.data.clone();
        let mut inner = FingerprintSipHasherBase::default();
        buffer.hash(&mut inner);
        let hash = inner.finish128();
        (
            Fingerprint {
                lo: hash.h1,
                hi: hash.h2,
            },
            buffer,
        )
    }
}

/// A fingerprint builder that produces unique fingerprint for each item.
/// It resolves the conflict by checking the underlying data.
/// See [`Fingerprint`] for more information.
#[derive(Default)]
pub struct FingerprintBuilder {
    /// The fast conflict checker mapping fingerprints to their underlying data.
    #[cfg(feature = "bi-hash")]
    fast_conflict_checker: crate::adt::CHashMap<u32, Vec<u8>>,
    /// The conflict checker mapping fingerprints to their underlying data.
    conflict_checker: crate::adt::CHashMap<Fingerprint, Vec<u8>>,
}

#[cfg(not(feature = "bi-hash"))]
impl FingerprintBuilder {
    /// Resolve the fingerprint without checking the conflict.
    pub fn resolve_unchecked<T: Hash>(&self, item: &T) -> Fingerprint {
        let mut s = FingerprintSipHasher { data: Vec::new() };
        item.hash(&mut s);
        let (fingerprint, _featured_data) = s.finish_fingerprint();
        fingerprint
    }

    /// Resolve the fingerprint and check the conflict.
    pub fn resolve<T: Hash + 'static>(&self, item: &T) -> Fingerprint {
        let mut s = FingerprintSipHasher { data: Vec::new() };
        item.type_id().hash(&mut s);
        item.hash(&mut s);

        let (fingerprint, featured_data) = s.finish_fingerprint();
        let Some(prev_featured_data) = self.conflict_checker.get(&fingerprint) else {
            self.conflict_checker.insert(fingerprint, featured_data);
            return fingerprint;
        };

        if *prev_featured_data == *featured_data {
            return fingerprint;
        }

        // todo: soft error
        panic!("Fingerprint conflict detected!");
    }
}

#[cfg(feature = "bi-hash")]
impl FingerprintBuilder {
    /// Resolve the fingerprint without checking the conflict.
    pub fn resolve_unchecked<T: Hash>(&self, item: &T) -> Fingerprint {
        let mut s = FingerprintSipHasher { data: Vec::new() };
        item.hash(&mut s);
        let (fingerprint, featured_data) = s.fast_hash();
        let Some(prev_featured_data) = self.fast_conflict_checker.get(&fingerprint) else {
            self.fast_conflict_checker.insert(fingerprint, s.data);
            return Fingerprint::from_pair(fingerprint as u64, 0);
        };

        if *prev_featured_data == *featured_data {
            return Fingerprint::from_pair(fingerprint as u64, 0);
        }

        let (fingerprint, _featured_data) = s.finish_fingerprint();
        fingerprint
    }

    /// Resolve the fingerprint and check the conflict.
    pub fn resolve<T: Hash + 'static>(&self, item: &T) -> Fingerprint {
        let mut s = FingerprintSipHasher { data: Vec::new() };
        item.type_id().hash(&mut s);
        item.hash(&mut s);
        let (fingerprint, featured_data) = s.fast_hash();
        let Some(prev_featured_data) = self.fast_conflict_checker.get(&fingerprint) else {
            self.fast_conflict_checker.insert(fingerprint, s.data);
            return Fingerprint::from_pair(fingerprint as u64, 0);
        };

        if *prev_featured_data == *featured_data {
            return Fingerprint::from_pair(fingerprint as u64, 0);
        }

        let (fingerprint, featured_data) = s.finish_fingerprint();
        let Some(prev_featured_data) = self.conflict_checker.get(&fingerprint) else {
            self.conflict_checker.insert(fingerprint, featured_data);
            return fingerprint;
        };

        if *prev_featured_data == *featured_data {
            return fingerprint;
        }

        // todo: soft error
        panic!("Fingerprint conflict detected!");
    }
}

/// This function provides a hash function for items, which also includes a type
/// id as part of the hash. Note: This function is not stable across different
/// versions of typst-ts, so it is preferred to be always used in memory.
/// Currently, this function use [`SipHasher13`] as the underlying hash
/// algorithm.
pub fn item_hash128<T: Hash + 'static>(item: &T) -> u128 {
    // Also hash the TypeId because the type might be converted
    // through an unsized coercion.
    let mut state = SipHasher13::new();
    item.type_id().hash(&mut state);
    item.hash(&mut state);
    state.finish128().as_u128()
}

/// Calculate a 128-bit siphash of a value.
/// Currently, this function use [`SipHasher13`] as the underlying hash
/// algorithm.
#[inline]
pub fn hash128<T: std::hash::Hash>(value: &T) -> u128 {
    let mut state = SipHasher13::new();
    value.hash(&mut state);
    state.finish128().as_u128()
}

/// A convenience function for when you need a quick 64-bit hash.
#[inline]
pub fn hash64<T: Hash + ?Sized>(v: &T) -> u64 {
    let mut state = FxHasher::default();
    v.hash(&mut state);
    state.finish()
}

// todo: rustc hash doesn't have 32-bit hash
pub use fxhash::hash32;

/// A trait that provides a static prehashed 128-bit hash.
pub trait StaticHash128 {
    /// Get the prehashed 128-bit hash.
    fn get_hash(&self) -> u128;
}

impl Hash for dyn StaticHash128 {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u128(self.get_hash());
    }
}

/// A trait that provides a static prehashed 64-bit hash for any internal `T`.
///
/// Please ensure that the `T` is really mapped to the hash. Use it at your own
/// risk.
pub struct HashedTrait<T: ?Sized> {
    hash: u128,
    t: Box<T>,
}

impl<T: ?Sized> HashedTrait<T> {
    /// Create a new `HashedTrait` with the given hash and the trait object.
    pub fn new(hash: u128, t: Box<T>) -> Self {
        Self { hash, t }
    }
}

impl<T: ?Sized> Deref for HashedTrait<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.t
    }
}

impl<T> Hash for HashedTrait<T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u128(self.hash);
    }
}

impl<T: Hash + Default + 'static> Default for HashedTrait<T> {
    fn default() -> Self {
        let t = T::default();
        Self {
            hash: item_hash128(&t),
            t: Box::new(t),
        }
    }
}

impl<T: ?Sized> StaticHash128 for HashedTrait<T> {
    fn get_hash(&self) -> u128 {
        self.hash
    }
}

#[test]
fn test_fingerprint() {
    let t = Fingerprint::from_pair(0, 1);
    assert_eq!(Fingerprint::try_from_str(&t.as_svg_id("")).unwrap(), t);

    let t = Fingerprint::from_pair(1, 1);
    assert_eq!(Fingerprint::try_from_str(&t.as_svg_id("")).unwrap(), t);

    let t = Fingerprint::from_pair(1, 0);
    assert_eq!(Fingerprint::try_from_str(&t.as_svg_id("")).unwrap(), t);

    let t = Fingerprint::from_pair(0, 0);
    assert_eq!(Fingerprint::try_from_str(&t.as_svg_id("")).unwrap(), t);
}
