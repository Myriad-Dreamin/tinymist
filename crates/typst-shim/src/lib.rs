//! # typst-shim

pub use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(feature = "nightly")] {
        mod nightly;
        pub use nightly::*;
    } else {
        mod stable;
        pub use stable::*;
    }
}
