//! Aggregate mock test support.
//!
//! The actual mock implementations live in their owning crates so those crates
//! can use the mocks in their own tests without dependency cycles:
//!
//! - VFS workspace/access/change helpers: `tinymist_vfs::mock`
//! - World/universe builders: `tinymist_world::mock`
//! - Project compiler event helpers: `tinymist_project::mock`
//!
//! This module re-exports those layers for consumers that already depend on the
//! aggregate `tinymist-tests` support crate.

pub use tinymist_project::mock::*;
pub use tinymist_vfs::mock::*;
pub use tinymist_world::mock::*;
