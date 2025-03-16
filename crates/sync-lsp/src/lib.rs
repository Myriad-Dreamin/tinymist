//! A synchronous language server implementation.

#[cfg(feature = "dap")]
pub mod dap;
#[cfg(feature = "lsp")]
pub mod lsp;
pub mod req_queue;
pub mod transport;

pub use error::*;
pub use msg::*;
pub use server::*;

mod error;
mod msg;
mod server;

use std::any::Any;
use std::pin::Pin;
use std::sync::{Arc, Weak};

use futures::future::MaybeDone;
use parking_lot::Mutex;
use reflexo::time::Instant;
use serde::Serialize;
use serde_json::Value as JsonValue;

type Event = Box<dyn Any + Send>;

/// The sender of the language server.
#[derive(Debug, Clone)]
pub struct TConnectionTx<M> {
    /// The sender of the events.
    pub event: crossbeam_channel::Sender<Event>,
    /// The sender of the LSP messages.
    pub lsp: crossbeam_channel::Sender<Message>,
    marker: std::marker::PhantomData<M>,
}

/// The sender of the language server.
#[derive(Debug, Clone)]
pub struct TConnectionRx<M> {
    /// The receiver of the events.
    pub event: crossbeam_channel::Receiver<Event>,
    /// The receiver of the LSP messages.
    pub lsp: crossbeam_channel::Receiver<Message>,
    marker: std::marker::PhantomData<M>,
}

impl<M: TryFrom<Message, Error = anyhow::Error>> TConnectionRx<M> {
    /// Receives a message or an event.
    fn recv(&self) -> anyhow::Result<EventOrMessage<M>> {
        crossbeam_channel::select_biased! {
            recv(self.lsp) -> msg => Ok(EventOrMessage::Msg(msg?.try_into()?)),
            recv(self.event) -> event => Ok(event.map(EventOrMessage::Evt)?),
        }
    }
}

/// The untyped connect tx for the language server.
pub type ConnectionTx = TConnectionTx<Message>;
/// The untyped connect rx for the language server.
pub type ConnectionRx = TConnectionRx<Message>;

/// This is a helper enum to handle both events and messages.
enum EventOrMessage<M> {
    Evt(Event),
    Msg(M),
}

/// Connection is just a pair of channels of LSP messages.
pub struct Connection<M> {
    /// The senders of the connection.
    pub sender: TConnectionTx<M>,
    /// The receivers of the connection.
    pub receiver: TConnectionRx<M>,
}

impl<M: TryFrom<Message, Error = anyhow::Error>> From<Connection<Message>> for Connection<M> {
    fn from(conn: Connection<Message>) -> Self {
        Self {
            sender: TConnectionTx {
                event: conn.sender.event,
                lsp: conn.sender.lsp,
                marker: std::marker::PhantomData,
            },
            receiver: TConnectionRx {
                event: conn.receiver.event,
                lsp: conn.receiver.lsp,
                marker: std::marker::PhantomData,
            },
        }
    }
}

impl<M: TryFrom<Message, Error = anyhow::Error>> From<TConnectionTx<M>> for ConnectionTx {
    fn from(conn: TConnectionTx<M>) -> Self {
        Self {
            event: conn.event,
            lsp: conn.lsp,
            marker: std::marker::PhantomData,
        }
    }
}

/// The common error type for the language server.
pub use msg::ResponseError;
/// The common result type for the language server.
pub type LspResult<T> = Result<T, ResponseError>;
/// A future that may be done in place or not.
pub type ResponseFuture<T> = MaybeDone<Pin<Box<dyn std::future::Future<Output = T> + Send>>>;
/// A future that may be rejected before actual started.
pub type LspResponseFuture<T> = LspResult<ResponseFuture<T>>;
/// A future that could be rejected by common error in `LspResponseFuture`.
pub type SchedulableResponse<T> = LspResponseFuture<LspResult<T>>;
/// The common future type for the language server.
pub type AnySchedulableResponse = SchedulableResponse<JsonValue>;
/// The result of a scheduled response which could be finally caught by
/// `schedule_tail`.
/// - Returns Ok(Some()) -> Already responded
/// - Returns Ok(None) -> Need to respond none
/// - Returns Err(..) -> Need to respond error
pub type ScheduledResult = LspResult<Option<()>>;

/// A helper function to create a `LspResponseFuture`
pub fn just_ok<T, E>(res: T) -> Result<ResponseFuture<Result<T, E>>, E> {
    Ok(futures::future::MaybeDone::Done(Ok(res)))
}
/// A helper function to create a `LspResponseFuture`
pub fn just_result<T, E>(res: Result<T, E>) -> Result<ResponseFuture<Result<T, E>>, E> {
    Ok(futures::future::MaybeDone::Done(res))
}
/// A helper function to create a `LspResponseFuture`
pub fn just_future<T, E>(
    fut: impl std::future::Future<Output = Result<T, E>> + Send + 'static,
) -> Result<ResponseFuture<Result<T, E>>, E> {
    Ok(futures::future::MaybeDone::Future(Box::pin(fut)))
}

/// Converts a `ScheduledResult` to a `SchedulableResponse`.
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
pub(crate) use reschedule;

type AnyCaster<S> = Arc<dyn Fn(&mut dyn Any) -> &mut S + Send + Sync>;

/// A Lsp client with typed service `S`.
pub struct TypedLspClient<S> {
    client: LspClient,
    caster: AnyCaster<S>,
}

impl<S> TypedLspClient<S> {
    /// Converts the client to an untyped client.
    pub fn to_untyped(self) -> LspClient {
        self.client
    }
}

impl<S: 'static> TypedLspClient<S> {
    /// Returns the untyped lsp client.
    pub fn untyped(&self) -> &LspClient {
        &self.client
    }

    /// Casts the service to another type.
    pub fn cast<T: 'static>(&self, f: fn(&mut S) -> &mut T) -> TypedLspClient<T> {
        let caster = self.caster.clone();
        TypedLspClient {
            client: self.client.clone(),
            caster: Arc::new(move |s| f(caster(s))),
        }
    }

    /// Sends a event to the client itself.
    pub fn send_event<T: std::any::Any + Send + 'static>(&self, event: T) {
        let Some(sender) = self.sender.upgrade() else {
            log::warn!("failed to send request: connection closed");
            return;
        };

        let Err(res) = sender.event.send(Box::new(event)) else {
            return;
        };
        log::warn!("failed to send event: {res:?}");
    }
}

impl<S> Clone for TypedLspClient<S> {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            caster: self.caster.clone(),
        }
    }
}

impl<S> std::ops::Deref for TypedLspClient<S> {
    type Target = LspClient;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

/// The root of the language server host.
/// Will close connection when dropped.
#[derive(Debug, Clone)]
pub struct LspClientRoot {
    weak: LspClient,
    _strong: Arc<ConnectionTx>,
}

impl LspClientRoot {
    /// Creates a new language server host.
    pub fn new<M: TryFrom<Message, Error = anyhow::Error> + GetMessageKind>(
        handle: tokio::runtime::Handle,
        sender: TConnectionTx<M>,
    ) -> Self {
        let _strong = Arc::new(sender.into());
        let weak = LspClient {
            handle,
            msg_kind: M::get_message_kind(),
            sender: Arc::downgrade(&_strong),
            req_queue: Arc::new(Mutex::new(ReqQueue::default())),
        };
        Self { weak, _strong }
    }

    /// Returns the weak reference to the language server host.
    pub fn weak(&self) -> LspClient {
        self.weak.clone()
    }
}

type ReqHandler = Box<dyn for<'a> FnOnce(&'a mut dyn Any, LspOrDapResponse) + Send + Sync>;
type ReqQueue = req_queue::ReqQueue<(String, Instant), ReqHandler>;

/// The host for the language server, or known as the LSP client.
#[derive(Debug, Clone)]
pub struct LspClient {
    /// The tokio handle.
    pub handle: tokio::runtime::Handle,

    msg_kind: MessageKind,
    sender: Weak<ConnectionTx>,
    req_queue: Arc<Mutex<ReqQueue>>,
}

impl LspClient {
    /// Returns the untyped lsp client.
    pub fn untyped(&self) -> &Self {
        self
    }

    /// converts the client to a typed client.
    pub fn to_typed<S: Any>(&self) -> TypedLspClient<S> {
        TypedLspClient {
            client: self.clone(),
            caster: Arc::new(|s| s.downcast_mut().expect("invalid cast")),
        }
    }

    /// Checks if there are pending requests.
    pub fn has_pending_requests(&self) -> bool {
        self.req_queue.lock().incoming.has_pending()
    }

    /// Prints states of the request queue and panics.
    pub fn begin_panic(&self) {
        self.req_queue.lock().begin_panic();
    }

    /// Sends a event to the server itself.
    pub fn send_event<T: std::any::Any + Send + 'static>(&self, event: T) {
        let Some(sender) = self.sender.upgrade() else {
            log::warn!("failed to send request: connection closed");
            return;
        };

        if let Err(res) = sender.event.send(Box::new(event)) {
            log::warn!("failed to send event: {res:?}");
        }
    }

    /// Completes an server2client request in the request queue.
    #[cfg(feature = "lsp")]
    pub fn complete_lsp_request<S: Any>(&self, service: &mut S, response: lsp::Response) {
        let mut req_queue = self.req_queue.lock();
        let Some(handler) = req_queue.outgoing.complete(response.id.clone()) else {
            log::warn!("received response for unknown request");
            return;
        };
        drop(req_queue);
        handler(service, response.into())
    }

    /// Completes an server2client request in the request queue.
    #[cfg(feature = "dap")]
    pub fn complete_dap_request<S: Any>(&self, service: &mut S, response: dap::Response) {
        let mut req_queue = self.req_queue.lock();
        let Some(handler) = req_queue
            .outgoing
            // todo: casting i64 to i32
            .complete((response.request_seq as i32).into())
        else {
            log::warn!("received response for unknown request");
            return;
        };
        drop(req_queue);
        handler(service, response.into())
    }

    /// Registers an client2server request in the request queue.
    pub fn register_request(&self, method: &str, id: &RequestId, received_at: Instant) {
        let mut req_queue = self.req_queue.lock();
        self.start_request(id, method);
        req_queue
            .incoming
            .register(id.clone(), (method.to_owned(), received_at));
    }

    /// Responds a typed result to the client.
    pub fn respond_result<T: Serialize>(&self, id: RequestId, result: LspResult<T>) {
        let result = result.and_then(|t| serde_json::to_value(t).map_err(internal_error));
        self.respond_any_result(id, result);
    }

    fn respond_any_result(&self, id: RequestId, result: LspResult<JsonValue>) {
        let req_id = id.clone();
        let msg: Message = match (self.msg_kind, result) {
            #[cfg(feature = "lsp")]
            (MessageKind::Lsp, Ok(resp)) => lsp::Response::new_ok(id, resp).into(),
            #[cfg(feature = "lsp")]
            (MessageKind::Lsp, Err(e)) => lsp::Response::new_err(id, e.code, e.message).into(),
            #[cfg(feature = "dap")]
            (MessageKind::Dap, Ok(resp)) => dap::Response::success(RequestId::dap(id), resp).into(),
            #[cfg(feature = "dap")]
            (MessageKind::Dap, Err(e)) => {
                dap::Response::error(RequestId::dap(id), Some(e.message), None).into()
            }
        };

        self.respond(req_id, msg);
    }

    /// Completes an client2server request in the request queue.
    pub fn respond(&self, id: RequestId, response: Message) {
        let mut req_queue = self.req_queue.lock();
        let Some((method, received_at)) = req_queue.incoming.complete(&id) else {
            return;
        };

        self.stop_request(&id, &method, received_at);

        let Some(sender) = self.sender.upgrade() else {
            log::warn!("failed to send response ({method}, {id}): connection closed");
            return;
        };
        if let Err(res) = sender.lsp.send(response) {
            log::warn!("failed to send response ({method}, {id}): {res:?}");
        }
    }
}

impl LspClient {
    /// Schedules a request from the client.
    pub fn schedule<T: Serialize + 'static>(
        &self,
        req_id: RequestId,
        resp: SchedulableResponse<T>,
    ) -> ScheduledResult {
        let resp = resp?;

        use futures::future::MaybeDone::*;
        match resp {
            Done(output) => {
                self.respond_result(req_id, output);
            }
            Future(fut) => {
                let client = self.clone();
                let req_id = req_id.clone();
                self.handle.spawn(async move {
                    client.respond_result(req_id, fut.await);
                });
            }
            Gone => {
                log::warn!("response for request({req_id:?}) already taken");
            }
        };

        Ok(Some(()))
    }

    /// Catch the early rejected requests.
    fn schedule_tail(&self, req_id: RequestId, resp: ScheduledResult) {
        match resp {
            // Already responded
            Ok(Some(())) => {}
            // The requests that doesn't start.
            _ => self.respond_result(req_id, resp),
        }
    }
}

impl LspClient {
    fn start_request(&self, req_id: &RequestId, method: &str) {
        log::info!("handling {method} - ({req_id})");
    }

    fn stop_request(&self, req_id: &RequestId, method: &str, received_at: Instant) {
        let duration = received_at.elapsed();
        log::info!("handled  {method} - ({req_id}) in {duration:0.2?}");
    }

    fn start_notification(&self, method: &str) {
        log::info!("notifying {method}");
    }

    fn stop_notification(&self, method: &str, received_at: Instant, result: LspResult<()>) {
        let request_duration = received_at.elapsed();
        if let Err(err) = result {
            log::error!("notify {method} failed in {request_duration:0.2?}: {err:?}");
        } else {
            log::info!("notify {method} succeeded in {request_duration:0.2?}");
        }
    }
}
