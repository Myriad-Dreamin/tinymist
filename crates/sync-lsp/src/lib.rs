use std::fmt::Display;
use std::pin::Pin;

use async_lsp::ResponseError;
use async_lsp::{ClientSocket, ErrorCode};
use lsp_types::notification::PublishDiagnostics;
use lsp_types::request::{RegisterCapability, UnregisterCapability};
use lsp_types::*;
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use tinymist_query::CompilerQueryResponse;

pub mod io;
pub mod lifecycle;
pub mod router;
pub mod transport;
pub use lifecycle::*;

pub type LspResult<Res> = Result<Res, ResponseError>;

/// Returns Ok(Some()) -> Already responded
/// Returns Ok(None) -> Need to respond none
/// Returns Err(..) -> Need to respond error
pub type ScheduledResult = LspResult<Option<()>>;

pub type ResponseFuture<T> = Pin<Box<dyn std::future::Future<Output = T> + Send>>;
pub type LspResponseFuture<T> = LspResult<ResponseFuture<T>>;
pub type QueryFuture = anyhow::Result<ResponseFuture<anyhow::Result<CompilerQueryResponse>>>;

pub type SchedulableResponse<T> = LspResponseFuture<LspResult<T>>;
pub type AnySchedulableResponse = SchedulableResponse<JsonValue>;

#[macro_export]
macro_rules! just_ok {
    ($expr:expr) => {
        Ok(Box::pin(std::future::ready(Ok($expr))))
    };
}

#[macro_export]
macro_rules! just_result {
    ($expr:expr) => {
        Ok(Box::pin(std::future::ready($expr)))
    };
}

#[macro_export]
macro_rules! just_future {
    ($expr:expr) => {
        Ok(Box::pin($expr))
    };
}

#[macro_export]
macro_rules! reschedule {
    ($expr:expr) => {
        match $expr {
            Ok(Some(())) => return,
            Ok(None) => Ok(futures::future::MaybeDone::Done(Ok(
                serde_json::Value::Null,
            ))),
            Err(e) => Err(e),
        }
    };
}

/// The host for the language server, or known as the LSP client.
#[derive(Debug, Clone)]
pub struct LspClient {
    /// The tokio handle.
    pub handle: tokio::runtime::Handle,
    /// The client socket.
    pub sender: ClientSocket,
}

impl LspClient {
    /// Creates a new language server host.
    pub fn new(handle: tokio::runtime::Handle, sender: ClientSocket) -> Self {
        Self { handle, sender }
    }

    pub fn has_pending_requests(&self) -> bool {
        // self.req_queue.lock().incoming.has_pending()
        todo!()
    }

    pub async fn send_request<R: lsp_types::request::Request>(
        &self,
        params: R::Params,
    ) -> async_lsp::Result<R::Result> {
        self.sender.request::<R>(params).await
    }

    pub fn send_notification<N: lsp_types::notification::Notification>(
        &self,
        params: N::Params,
    ) -> async_lsp::Result<()> {
        self.sender.notify::<N>(params)
    }

    pub fn publish_diagnostics(
        &self,
        uri: Url,
        diagnostics: Vec<Diagnostic>,
        version: Option<i32>,
    ) -> async_lsp::Result<()> {
        self.send_notification::<PublishDiagnostics>(PublishDiagnosticsParams {
            uri,
            diagnostics,
            version,
        })
    }

    pub async fn register_capability(
        &self,
        registrations: Vec<Registration>,
    ) -> async_lsp::Result<()> {
        self.send_request::<RegisterCapability>(RegistrationParams { registrations })
            .await
    }

    pub async fn unregister_capability(
        &self,
        unregisterations: Vec<Unregistration>,
    ) -> async_lsp::Result<()> {
        self.send_request::<UnregisterCapability>(UnregistrationParams { unregisterations })
            .await
    }
}

impl LspClient {}

pub fn from_json<T: DeserializeOwned>(
    what: &'static str,
    json: &serde_json::Value,
) -> anyhow::Result<T> {
    serde_json::from_value(json.clone())
        .map_err(|e| anyhow::anyhow!("Failed to deserialize {what}: {e}; {json}"))
}

pub fn invalid_params(message: impl Display) -> ResponseError {
    ResponseError::new(ErrorCode::INVALID_PARAMS, message)
}

pub fn internal_error(message: impl Display) -> ResponseError {
    ResponseError::new(ErrorCode::INTERNAL_ERROR, message)
}

pub fn method_not_found() -> ResponseError {
    ResponseError::new(ErrorCode::METHOD_NOT_FOUND, "Method not found")
}
