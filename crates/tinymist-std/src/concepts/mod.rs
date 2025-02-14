mod takable;
use std::{path::Path, sync::Arc};

pub use takable::*;

pub mod cow_mut;

mod query;
pub use query::*;

mod read;
pub use read::*;

mod marker;
pub use marker::*;

#[cfg(feature = "typst")]
pub mod typst;

/// An immutable string.
pub type ImmutStr = Arc<str>;
/// An immutable byte slice.
pub type ImmutBytes = Arc<[u8]>;
/// An immutable path.
pub type ImmutPath = Arc<Path>;

/// A trait for converting an `Arc<T>` into `Self`.
pub trait FromArc<T> {
    /// Converts an `Arc<T>` into `Self`.
    fn from_arc(arc: Arc<T>) -> Self;
}

impl<S, T> FromArc<S> for T
where
    Arc<S>: Into<T>,
{
    fn from_arc(arc: Arc<S>) -> T {
        arc.into()
    }
}

/// A trait for converting `Arc<T>` into `Self`.
pub trait ArcInto<T> {
    /// Converts `Arc<T>` into `Self`.
    fn arc_into(self: Arc<Self>) -> T;
}

impl<S, T> ArcInto<T> for S
where
    Arc<S>: Into<T>,
{
    fn arc_into(self: Arc<Self>) -> T {
        self.into()
    }
}
