//! Tinymist Analysis

pub mod adt;
pub mod docs;
pub mod location;
mod sig;
pub mod stats;
pub mod syntax;
pub mod ty;
pub mod upstream;

pub use sig::*;
pub use track_values::*;

mod prelude;
mod track_values;

/// Completely disabled log
#[macro_export]
macro_rules! log_debug_ct_ {
    // debug!(target: "my_target", key1 = 42, key2 = true; "a {} event", "log")
    // debug!(target: "my_target", "a {} event", "log")
    (target: $target:expr, $($arg:tt)+) => {
        let _ = format_args!($target, $($arg)+);
    };

    // debug!("a {} event", "log")
    ($($arg:tt)+) => {
        let _ = format_args!($($arg)+);
    };
}
pub use log_debug_ct_ as log_debug_ct;
