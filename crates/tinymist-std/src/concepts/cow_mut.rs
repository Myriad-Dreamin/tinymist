//! <https://github.com/rust-analyzer/rowan/blob/v0.16.1/src/cow_mut.rs>
//!
//! This module provides a `CowMut` type, which is a mutable version of `Cow`.
//! Although it is strange that we can have a `CowMut`, because it should "copy
//! on write", we also don't love the `Cow` API and use `Cow` without even
//! touching its `DerefMut` feature.

/// A mutable version of [Cow][`std::borrow::Cow`].
#[derive(Debug)]
pub enum CowMut<'a, T> {
    /// An owned data.
    Owned(T),
    /// A borrowed mut data.
    Borrowed(&'a mut T),
}

impl<T> std::ops::Deref for CowMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            CowMut::Owned(it) => it,
            CowMut::Borrowed(it) => it,
        }
    }
}

impl<T> std::ops::DerefMut for CowMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        match self {
            CowMut::Owned(it) => it,
            CowMut::Borrowed(it) => it,
        }
    }
}

impl<T: Default> Default for CowMut<'_, T> {
    fn default() -> Self {
        CowMut::Owned(T::default())
    }
}
