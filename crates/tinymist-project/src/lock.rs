/// The default name of the lock file used by tinymist.
pub const LOCK_FILENAME: &str = "tinymist.lock";

#[cfg(feature = "system")]
mod system;
#[cfg(feature = "system")]
pub use system::*;
