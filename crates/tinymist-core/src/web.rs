//! Tinymist Web APIs.

use wasm_bindgen::prelude::*;

use crate::LONG_VERSION;

/// Gets the long version description of the library.
#[wasm_bindgen]
pub fn version() -> String {
    LONG_VERSION.clone()
}
