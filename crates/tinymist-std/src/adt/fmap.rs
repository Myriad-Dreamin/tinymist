//! A map that shards items by their fingerprint.

use std::{collections::HashMap, num::NonZeroU32};

use crate::hash::Fingerprint;

/// A global upper bound on the shard size.
/// If there are too many shards, the memory overhead is unacceptable.
const MAX_SHARD_SIZE: u32 = 512;

/// Return a read-only default shard size.
fn default_shard_size() -> NonZeroU32 {
    static ITEM_SHARD_SIZE: std::sync::OnceLock<NonZeroU32> = std::sync::OnceLock::new();

    /// By testing, we found that the optimal shard size is 2 * number of
    /// threads.
    fn determine_default_shard_size() -> NonZeroU32 {
        // This detection is from rayon.
        let thread_cnt = {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
        };

        // A valid shard size is a power of two.
        let size = (thread_cnt.next_power_of_two() * 2) as u32;
        // Perform early non-zero check to avoid panics.
        NonZeroU32::new(size.min(MAX_SHARD_SIZE)).unwrap()
    }

    *ITEM_SHARD_SIZE.get_or_init(determine_default_shard_size)
}

type FMapBase<V> = parking_lot::RwLock<HashMap<Fingerprint, V>>;

/// A map that shards items by their fingerprint. This is faster
/// than the dashmap in some cases.
///
/// It is fast since a fingerprint could split items into different shards
/// efficiently.
///
/// Note: If a fingerprint is not calculated from a hash function, it is not
/// guaranteed that the fingerprint is evenly distributed. Thus, in that case,
/// the performance of this map is not guaranteed.
pub struct FingerprintMap<V> {
    mask: u32,
    shards: Vec<parking_lot::RwLock<HashMap<Fingerprint, V>>>,
}

impl<V> Default for FingerprintMap<V> {
    fn default() -> Self {
        Self::new(default_shard_size())
    }
}

impl<V> FingerprintMap<V> {
    /// Create a new `FingerprintMap` with the given shard size.
    pub fn new(shard_size: NonZeroU32) -> Self {
        let shard_size = shard_size.get().next_power_of_two();
        let shard_size = shard_size.min(MAX_SHARD_SIZE);

        assert!(
            shard_size.is_power_of_two(),
            "shard size must be a power of two"
        );
        assert!(shard_size > 0, "shard size must be greater than zero");
        Self {
            mask: shard_size - 1,
            shards: (0..shard_size)
                .map(|_| parking_lot::RwLock::new(HashMap::new()))
                .collect(),
        }
    }

    /// Iterate over all items in the map.
    pub fn into_items(self) -> impl Iterator<Item = (Fingerprint, V)> {
        self.shards
            .into_iter()
            .flat_map(|shard| shard.into_inner().into_iter())
    }

    /// Get the shard
    pub fn shard(&self, fg: Fingerprint) -> &FMapBase<V> {
        let shards = &self.shards;
        let route_idx = (fg.lower32() & self.mask) as usize;

        // check that the route index is within the bounds of the shards
        debug_assert!(route_idx < shards.len());
        // SAFETY: `fg` is a valid index into `shards`, as shards size is never changed
        // and mask is always a power of two.
        unsafe { shards.get_unchecked(route_idx) }
    }

    /// Useful for parallel iteration
    pub fn as_mut_slice(&mut self) -> &mut [FMapBase<V>] {
        &mut self.shards
    }

    /// Checks if the map is empty.
    pub fn contains_key(&self, fg: &Fingerprint) -> bool {
        self.shard(*fg).read().contains_key(fg)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_default_shard_size() {
        let size = super::default_shard_size().get();

        eprintln!("size = {size}");

        assert!(size > 0);
        assert_eq!(size & (size - 1), 0);
    }
}
