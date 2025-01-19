#![allow(missing_docs)]

pub mod adt;
pub mod debug_loc;
pub mod error;
pub mod hash;
pub mod path;
pub mod time;

pub(crate) mod concepts;

pub use concepts::*;

pub use error::{ErrKind, Error};

#[cfg(feature = "typst")]
pub use typst_shim;

#[cfg(feature = "rkyv")]
use rkyv::{Archive, Deserialize as rDeser, Serialize as rSer};

/// The local id of a svg item.
/// This id is only unique within the svg document.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct DefId(pub u64);
