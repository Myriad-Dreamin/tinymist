//! Tinymist Web APIs.

#![allow(unused)]

use js_sys::{Function, Promise};
use wasm_bindgen::prelude::*;

use crate::LONG_VERSION;

/// Gets the long version description of the library.
#[wasm_bindgen]
pub fn version() -> String {
    LONG_VERSION.clone()
}

/// The Tinymist Language Server for WebAssembly.
#[wasm_bindgen]
pub struct TinymistLanguageServer {
    send_diagnostics: Function,
    send_request: Function,
    send_notification: Function,
}

#[wasm_bindgen]
impl TinymistLanguageServer {
    /// Creates a new instance of the Tinymist Language Server.
    #[wasm_bindgen(constructor)]
    pub fn new(
        send_diagnostics: Function,
        send_request: Function,
        send_notification: Function,
    ) -> Self {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));

        Self {
            send_diagnostics,
            send_request,
            send_notification,
        }
    }

    /// Handles incoming requests.
    pub fn on_request(&self, method: String, js_params: JsValue) -> Result<JsValue, JsValue> {
        todo!()
    }

    /// Handles incoming notifications.
    pub fn on_notification(&self, method: String, js_params: JsValue) -> Promise {
        todo!()
    }
}
