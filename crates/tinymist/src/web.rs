//! Tinymist Web APIs.

#![allow(unused)]

use futures::future::MaybeDone;
use js_sys::{Array, Function, Object, Promise};
use lsp_types::{DocumentSymbol, DocumentSymbolResponse};
use std::{collections::HashMap, sync::LazyLock};
use sync_ls::{
    erased_response, internal_error, invalid_params, lsp, GetMessageKind, JsTransportSender,
    LsDriver, LspBuilder, LspClientRoot, LspMessage, Message, ResponseError, TConnectionTx,
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
    version: String,
    state: LsDriver<LspMessage, RegularInit>,
}

#[wasm_bindgen]
impl TinymistLanguageServer {
    /// Creates a new language server.
    #[wasm_bindgen(constructor)]
    pub fn new(send_event: Function, send_request: Function, send_notification: Function) -> Self {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));

        let client = client_root(send_event, send_request, send_notification);
        let state = ServerState::install_lsp(LspBuilder::new(
            RegularInit {
                client: client.weak().to_typed(),
                font_opts: CompileFontArgs::default(),
                exec_cmds: Vec::new(),
            },
            client.weak(),
        ))
        .build();
        // .start(conn.receiver, is_replay)

        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            state,
        }
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

    /// Get the version of the language server.
    pub fn version(&self) -> String {
        self.version.clone()
    }

    /// Get a greeting message.
    pub fn greet(&self) -> String {
        format!("Hello from Tinymist WASM v{}!", self.version)
    }
}

/// Creates a new language server host.
fn client_root(
    send_event: Function,
    send_request: Function,
    send_notification: Function,
) -> LspClientRoot {
    LspClientRoot::new_js(
        RUNTIMES.tokio_runtime.handle().clone(),
        JsTransportSender::new(send_event, send_request, send_notification),
    )
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
