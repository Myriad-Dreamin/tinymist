//! Typst utils
pub use typst::utils::*;

/// Round a value to two decimal places.
pub fn round_2(value: f64) -> f64 {
    round_with_precision(value, 2)
}
