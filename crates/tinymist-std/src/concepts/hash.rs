//！todo: move to core/src/hash.rs

use std::{
    hash::{Hash, Hasher},
    ops::Deref,
};

use crate::hash::item_hash128;

pub trait StaticHash128 {
    fn get_hash(&self) -> u128;
}

impl Hash for dyn StaticHash128 {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u128(self.get_hash());
    }
}

pub struct HashedTrait<T: ?Sized> {
    hash: u128,
    t: Box<T>,
}

impl<T: ?Sized> HashedTrait<T> {
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
