//! Tinymist Web APIs.

#![allow(unused)]

use std::sync::LazyLock;

use futures::future::MaybeDone;
use js_sys::{Function, Promise};
use sync_ls::{
    internal_error, invalid_params, JsTransportSender, LsDriver, LspBuilder, LspClientRoot,
    LspMessage, ResponseError,
};
use tinymist_project::CompileFontArgs;
use wasm_bindgen::prelude::*;

use crate::{RegularInit, ServerState, LONG_VERSION};

/// Gets the long version description of the library.
#[wasm_bindgen]
pub fn version() -> String {
    LONG_VERSION.clone()
}

/// TinymistLanguageServer implements the LSP protocol for Typst documents
/// in a WebAssembly environment
#[wasm_bindgen]
pub struct TinymistLanguageServer {
    /// The client root that strongly references the LSP client.
    _client: LspClientRoot,
    /// The mutable state of the server.
    state: LsDriver<LspMessage, RegularInit>,
}

#[wasm_bindgen]
impl TinymistLanguageServer {
    /// Creates a new language server.
    #[wasm_bindgen(constructor)]
    pub fn new(init_opts: JsValue) -> Result<Self, JsValue> {
        let sender = serde_wasm_bindgen::from_value::<JsTransportSender>(init_opts)
            .map_err(|err| JsValue::from_str(&format!("Failed to deserialize init opts: {err}")))?;

        std::panic::set_hook(Box::new(console_error_panic_hook::hook));

        let _client = LspClientRoot::new_js(RUNTIMES.tokio_runtime.handle().clone(), sender);
        // Starts logging
        let _ = crate::init_log(crate::InitLogOpts {
            is_transient_cmd: false,
            is_test_no_verbose: false,
            output: Some(_client.weak()),
        });
        let state = ServerState::install_lsp(LspBuilder::new(
            RegularInit {
                client: _client.weak().to_typed(),
                font_opts: CompileFontArgs::default(),
                exec_cmds: Vec::new(),
            },
            _client.weak(),
        ))
        .build();

        Ok(Self { _client, state })
    }

    /// Handles internal events.
    pub fn on_event(&mut self, event_id: u32) {
        self.state.on_server_event(event_id);
    }

    /// Handles incoming requests.
    pub fn on_request(&mut self, method: String, js_params: JsValue) -> JsValue {
        let params = serde_wasm_bindgen::from_value::<serde_json::Value>(js_params);
        let params = match params {
            Ok(p) => p,
            Err(err) => return lsp_err(invalid_params(err)),
        };

        let result = self.state.on_lsp_request(&method, params);

        match result {
            Ok(MaybeDone::Done(Ok(t))) => lsp_serialize(&t),
            Ok(MaybeDone::Done(Err(err))) => lsp_err(err),
            // tokio doesn't get scheduled anymore after returning to js world
            Ok(MaybeDone::Future(fut)) => wasm_bindgen_futures::future_to_promise(async move {
                Ok(match fut.await {
                    Ok(t) => lsp_serialize(&t),
                    Err(err) => lsp_err(err),
                })
            })
            .into(),
            // match futures::executor::block_on(fut) {
            //     Ok(t) => lsp_serialize(&t),
            //     Err(err) => lsp_err(err),
            // },
            Ok(MaybeDone::Gone) => lsp_err(internal_error("response was weirdly gone")),
            Err(err) => lsp_err(err),
        }
    }

    /// Handles incoming notifications.
    pub fn on_notification(&mut self, method: String, js_params: JsValue) {
        let params = serde_wasm_bindgen::from_value::<serde_json::Value>(js_params);
        let params = match params {
            Ok(p) => p,
            Err(err) => {
                log::error!("Failed to deserialize notification params: {err}");
                return;
            }
        };

        let err = self.state.on_notification(&method, params);
        if let Err(err) = err {
            log::error!("Failed to handle notification {method}: {err:?}");
        }
    }

    /// Handles incoming responses.
    pub fn on_response(&mut self, js_result: JsValue) {
        let result = serde_wasm_bindgen::from_value::<sync_ls::lsp::Response>(js_result);
        let resp = match result {
            Ok(r) => r,
            Err(err) => {
                log::error!("Failed to deserialize response: {err}");
                return;
            }
        };
        self.state.on_lsp_response(resp);
    }

    /// Get the version of the language server.
    pub fn version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

/// The runtimes used by the application.
pub struct Runtimes {
    /// The tokio runtime.
    pub tokio_runtime: tokio::runtime::Runtime,
}

impl Default for Runtimes {
    fn default() -> Self {
        let tokio_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        Self { tokio_runtime }
    }
}

static RUNTIMES: LazyLock<Runtimes> = LazyLock::new(Runtimes::default);

fn lsp_err(err: ResponseError) -> JsValue {
    to_js_value(&err).unwrap()
}

fn lsp_serialize<T: serde::Serialize>(value: &T) -> JsValue {
    match to_js_value(value) {
        Ok(v) => v,
        Err(err) => lsp_err(internal_error(err.to_string())),
    }
}

// todo: poor performance, struct -> serde_json -> serde_wasm_bindgen ->
// serialize -> deserialize??
fn to_js_value<T: serde::Serialize>(value: &T) -> Result<JsValue, serde_wasm_bindgen::Error> {
    value.serialize(&serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true))
}
