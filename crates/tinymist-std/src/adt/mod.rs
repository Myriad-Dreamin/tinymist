pub mod fmap;
pub use fmap::FingerprintMap;

// todo: remove it if we could find a better alternative
pub use dashmap::DashMap as CHashMap;
