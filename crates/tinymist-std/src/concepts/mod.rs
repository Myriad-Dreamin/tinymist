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

/// An immutable string.
pub type ImmutStr = Arc<str>;
/// An immutable byte slice.
pub type ImmutBytes = Arc<[u8]>;
/// An immutable path.
pub type ImmutPath = Arc<Path>;
