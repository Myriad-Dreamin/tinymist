//! The font implementations for typst worlds.
//!
//! The core concept is the [`FontResolver`], implemented by
//! [`FontResolverImpl`].
//!
//! You can construct a [`FontResolverImpl`] on systems, browsers or simply
//! without touching any external environment. See the [`system`], [`web`] and
//! [`pure`] crates for more details.
//!
//! The [`FontResolverImpl`] has a lot of [`FontSlot`] objects and allow to load
//! font resources lazily.
//!
//! There are also other structs, which help store and load [`FontInfo`] objects
//! in the local file system or the remote machine. See the [`cache`] and
//! [`profile`] crates for more details.

pub mod cache;
pub(crate) mod incr;
pub(crate) mod info;
pub(crate) mod loader;
pub(crate) mod profile;
pub(crate) mod resolver;
pub(crate) mod slot;

pub use loader::*;
pub use profile::*;
pub use resolver::*;
pub use slot::*;

#[cfg(feature = "system")]
pub mod system;

#[cfg(feature = "web")]
pub mod web;

pub mod memory;
#[deprecated(note = "use memory module instead")]
pub use memory as pure;
