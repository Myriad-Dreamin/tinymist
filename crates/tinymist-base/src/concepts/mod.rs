mod takable;
use std::{path::Path, sync::Arc};

pub use takable::*;

mod hash;
pub use hash::*;

pub mod cow_mut;

mod query;
pub use query::*;

mod read;
pub use read::*;

mod marker;
pub use marker::*;

pub type ImmutStr = Arc<str>;
pub type ImmutBytes = Arc<[u8]>;
pub type ImmutPath = Arc<Path>;
