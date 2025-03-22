use core::fmt;
use std::num::NonZeroUsize;

/// A type that can be used to index into an IndexVec.
#[derive(Debug, PartialEq, Eq)]
pub struct IndexVecIdx<T>(NonZeroUsize, std::marker::PhantomData<T>);

impl<T> IndexVecIdx<T> {
    fn from_usize(id: usize) -> IndexVecIdx<T> {
        IndexVecIdx(
            NonZeroUsize::new(id + 1).expect("overflow"),
            std::marker::PhantomData,
        )
    }

    fn as_index(self) -> usize {
        self.0.get() - 1
    }
}

impl<T> fmt::Display for IndexVecIdx<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_index())
    }
}

impl<T> Clone for IndexVecIdx<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for IndexVecIdx<T> {}

impl<T> std::hash::Hash for IndexVecIdx<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

/// A vector that can be indexed by a usize.
#[derive(Debug, Clone, Hash)]
pub struct IndexVec<T> {
    vec: Vec<T>,
}

/// Implementation of IndexVec.
impl<T> IndexVec<T> {
    /// Creates a new IndexVec.
    pub fn new() -> Self {
        Self { vec: vec![] }
    }

    /// Pushes a new element to the IndexVec.
    pub fn push(&mut self, data: T) -> IndexVecIdx<T> {
        let id = self.vec.len();
        self.vec.push(data);
        IndexVecIdx::from_usize(id)
    }

    /// Gets the element at the given index.
    pub fn get(&self, id: IndexVecIdx<T>) -> &T {
        &self.vec[id.as_index()]
    }

    /// Gets a mutable reference to the element at the given index.
    pub fn get_mut(&mut self, id: IndexVecIdx<T>) -> &mut T {
        &mut self.vec[id.as_index()]
    }
}

impl<'a, T> IntoIterator for &'a IndexVec<T> {
    type Item = (IndexVecIdx<T>, &'a T);
    type IntoIter = IndexVecIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        IndexVecIter {
            iter: self.vec.iter().enumerate(),
            _marker: std::marker::PhantomData,
        }
    }
}

/// An iterator for IndexVec.
pub struct IndexVecIter<'a, T> {
    iter: std::iter::Enumerate<std::slice::Iter<'a, T>>,
    _marker: std::marker::PhantomData<T>,
}

impl<'a, T> Iterator for IndexVecIter<'a, T> {
    type Item = (IndexVecIdx<T>, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter
            .next()
            .map(|(i, v)| (IndexVecIdx::from_usize(i), v))
    }
}

/// Trait for converting an index to a type.
pub trait FromIndex {
    /// Converts an index to a type.
    fn from_index(id: IndexVecIdx<Self>) -> Self
    where
        Self: std::marker::Sized;
}

/// Implementation of IndexVec.
impl<T: FromIndex> IndexVec<T> {
    /// Creates a mutable reference to the element.
    pub fn create(&mut self) -> &mut T {
        let id = IndexVecIdx::from_usize(self.vec.len());
        self.vec.push(T::from_index(id));
        &mut self.vec[id.as_index()]
    }
}

impl<T> Default for IndexVec<T> {
    fn default() -> Self {
        Self::new()
    }
}
