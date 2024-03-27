use std::sync::Arc;
use std::time::Instant;

use log::{info, trace, warn};
use lsp_types::InitializedParams;
use parking_lot::RwLock;
use serde::{de::DeserializeOwned, Serialize};

use lsp_server::{Connection, Message, Response};

use lsp_types::notification::PublishDiagnostics;
use lsp_types::request::{RegisterCapability, UnregisterCapability};
use lsp_types::*;
use parking_lot::Mutex;

// Enforces drop order
pub struct Handle<H, C> {
    pub handle: H,
    pub receiver: C,
}

pub type ReqHandler<S> = for<'a> fn(&'a mut S, lsp_server::Response);
type ReqQueue<S> = lsp_server::ReqQueue<(String, Instant), ReqHandler<S>>;

/// The host for the language server, or known as the LSP client.
#[derive(Debug)]
pub struct LspHost<S> {
    sender: Arc<RwLock<Option<crossbeam_channel::Sender<Message>>>>,
    req_queue: Arc<Mutex<ReqQueue<S>>>,
}

impl<S> Clone for LspHost<S> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            req_queue: self.req_queue.clone(),
        }
    }
}

impl<S> LspHost<S> {
    /// Creates a new language server host.
    pub fn new(sender: Arc<RwLock<Option<crossbeam_channel::Sender<Message>>>>) -> Self {
        Self {
            sender,
            req_queue: Arc::new(Mutex::new(ReqQueue::default())),
        }
    }

    pub fn send_request<R: lsp_types::request::Request>(
        &self,
        params: R::Params,
        handler: ReqHandler<S>,
    ) {
        let mut req_queue = self.req_queue.lock();
        let sender = self.sender.read();
        let Some(sender) = sender.as_ref() else {
            warn!("closed connection, failed to send request");
            return;
        };
        let request = req_queue
            .outgoing
            .register(R::METHOD.to_owned(), params, handler);
        let Err(res) = sender.send(request.into()) else {
            return;
        };
        warn!("failed to send request: {res:?}");
    }

    pub fn complete_request(&self, service: &mut S, response: lsp_server::Response) {
        let mut req_queue = self.req_queue.lock();
        let Some(handler) = req_queue.outgoing.complete(response.id.clone()) else {
            warn!("received response for unknown request");
            return;
        };
        drop(req_queue);
        handler(service, response)
    }

    pub fn send_notification<N: lsp_types::notification::Notification>(&self, params: N::Params) {
        let not = lsp_server::Notification::new(N::METHOD.to_owned(), params);

        let sender = self.sender.read();
        let Some(sender) = sender.as_ref() else {
            warn!("closed connection, failed to send request");
            return;
        };
        let Err(res) = sender.send(not.into()) else {
            return;
        };
        warn!("failed to send notification: {res:?}");
    }

    pub fn register_request(&self, request: &lsp_server::Request, request_received: Instant) {
        let mut req_queue = self.req_queue.lock();
        info!(
            "handling {} - ({}) at {:0.2?}",
            request.method, request.id, request_received
        );
        req_queue.incoming.register(
            request.id.clone(),
            (request.method.clone(), request_received),
        );
    }
    pub fn respond(&self, response: lsp_server::Response) {
        let mut req_queue = self.req_queue.lock();
        if let Some((method, start)) = req_queue.incoming.complete(response.id.clone()) {
            let sender = self.sender.read();
            let Some(sender) = sender.as_ref() else {
                warn!("closed connection, failed to send request");
                return;
            };

            // if let Some(err) = &response.error {
            //     if err.message.starts_with("server panicked") {
            //         self.poke_rust_analyzer_developer(format!("{}, check the log",
            // err.message))     }
            // }

            let duration = start.elapsed();
            info!(
                "handled  {} - ({}) in {:0.2?}",
                method, response.id, duration
            );
            let Err(res) = sender.send(response.into()) else {
                return;
            };
            warn!("failed to send response: {res:?}");
        }
    }

    pub fn publish_diagnostics(
        &self,
        uri: Url,
        diagnostics: Vec<Diagnostic>,
        version: Option<i32>,
    ) {
        self.send_notification::<PublishDiagnostics>(PublishDiagnosticsParams {
            uri,
            diagnostics,
            version,
        });
    }

    // todo: handle error
    pub fn register_capability(&self, registrations: Vec<Registration>) -> anyhow::Result<()> {
        self.send_request::<RegisterCapability>(RegistrationParams { registrations }, |_, resp| {
            if let Some(err) = resp.error {
                log::error!("failed to register capability: {err:?}");
            }
        });
        Ok(())
    }

    pub fn unregister_capability(
        &self,
        unregisterations: Vec<Unregistration>,
    ) -> anyhow::Result<()> {
        self.send_request::<UnregisterCapability>(
            UnregistrationParams { unregisterations },
            |_, resp| {
                if let Some(err) = resp.error {
                    log::error!("failed to unregister capability: {err:?}");
                }
            },
        );
        Ok(())
    }
}

pub trait LspDriver {
    type InitParams: DeserializeOwned;
    type InitResult: Serialize;
    type InitializedSelf: InitializedLspDriver;

    fn initialize(
        self,
        host: LspHost<Self::InitializedSelf>,
        params: Self::InitParams,
    ) -> (
        Self::InitializedSelf,
        Result<Self::InitResult, lsp_server::ResponseError>,
    );
}

pub trait InitializedLspDriver {
    fn initialized(&mut self, params: InitializedParams);
    fn main_loop(&mut self, receiver: crossbeam_channel::Receiver<Message>) -> anyhow::Result<()>;
}

pub fn lsp_harness<D: LspDriver>(
    driver: D,
    connection: Connection,
    force_exit: &mut bool,
) -> anyhow::Result<()> {
    *force_exit = false;
    // todo: ugly code
    let (initialize_id, initialize_params) = match connection.initialize_start() {
        Ok(it) => it,
        Err(e) => {
            log::error!("failed to initialize: {e}");
            *force_exit = !e.channel_is_disconnected();
            return Err(e.into());
        }
    };
    let request_received = std::time::Instant::now();
    trace!("InitializeParams: {initialize_params}");
    let sender = Arc::new(RwLock::new(Some(connection.sender)));
    let host = LspHost::new(sender.clone());

    let _drop_connection = ForceDrop(sender);

    let req = lsp_server::Request::new(initialize_id, "initialize".to_owned(), initialize_params);
    host.register_request(&req, request_received);
    let lsp_server::Request {
        id: initialize_id,
        params: initialize_params,
        ..
    } = req;

    let initialize_params = from_json::<D::InitParams>("InitializeParams", &initialize_params)?;

    let (mut service, initialize_result) = driver.initialize(host.clone(), initialize_params);

    host.respond(match initialize_result {
        Ok(cap) => Response::new_ok(initialize_id, Some(cap)),
        Err(err) => Response::new_err(initialize_id, err.code, err.message),
    });

    info!("waiting for initialized notification");
    let initialized_ack = match &connection.receiver.recv() {
        Ok(Message::Notification(n)) if n.method == "initialized" => Ok(()),
        Ok(msg) => Err(ProtocolError::new(format!(
            r#"expected initialized notification, got: {msg:?}"#
        ))),
        Err(e) => {
            log::error!("failed to receive initialized notification: {e}");
            Err(ProtocolError::disconnected())
        }
    };
    if let Err(e) = initialized_ack {
        *force_exit = !e.channel_is_disconnected();
        return Err(anyhow::anyhow!(
            "failed to receive initialized notification: {e:?}"
        ));
    }

    service.initialized(InitializedParams {});
    service.main_loop(connection.receiver)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProtocolError(String, bool);

impl ProtocolError {
    pub(crate) fn new(msg: impl Into<String>) -> Self {
        ProtocolError(msg.into(), false)
    }

    pub(crate) fn disconnected() -> ProtocolError {
        ProtocolError("disconnected channel".into(), true)
    }

    /// Whether this error occured due to a disconnected channel.
    pub fn channel_is_disconnected(&self) -> bool {
        self.1
    }
}

struct ForceDrop<T>(Arc<RwLock<Option<T>>>);
impl<T> Drop for ForceDrop<T> {
    fn drop(&mut self) {
        self.0.write().take();
    }
}

pub fn from_json<T: DeserializeOwned>(
    what: &'static str,
    json: &serde_json::Value,
) -> anyhow::Result<T> {
    serde_json::from_value(json.clone())
        .map_err(|e| anyhow::format_err!("Failed to deserialize {what}: {e}; {json}"))
}
