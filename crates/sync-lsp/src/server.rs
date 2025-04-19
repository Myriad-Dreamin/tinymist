//! A synchronous language server implementation.

#[cfg(feature = "dap")]
mod dap_srv;

#[cfg(feature = "lsp")]
mod lsp_srv;

use core::fmt;
use std::any::Any;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Weak};
use std::time::Instant;

use futures::future::MaybeDone;
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{from_value, Value as JsonValue};

#[cfg(feature = "lsp")]
use crate::lsp::{Notification, Request};
use crate::msg::*;
use crate::req_queue;
use crate::*;

type ImmutPath = Arc<Path>;

/// A future that may be done in place or not.
pub type ResponseFuture<T> = MaybeDone<Pin<Box<dyn std::future::Future<Output = T> + Send>>>;
/// A future that may be rejected before actual started.
pub type LspResponseFuture<T> = LspResult<ResponseFuture<T>>;
/// A future that could be rejected by common error in `LspResponseFuture`.
pub type SchedulableResponse<T> = LspResponseFuture<LspResult<T>>;
/// The common response future type for language servers.
pub type AnySchedulableResponse = SchedulableResponse<JsonValue>;
/// The result of a scheduled response which could be finally caught by
/// `schedule_tail`.
/// - Returns Ok(Some()) -> Already responded
/// - Returns Ok(None) -> Need to respond none
/// - Returns Err(..) -> Need to respond error
pub type ScheduledResult = LspResult<Option<()>>;

/// The untyped connect tx for language servers.
pub type ConnectionTx = TConnectionTx<Message>;
/// The untyped connect rx for language servers.
pub type ConnectionRx = TConnectionRx<Message>;

/// The sender of the language server.
#[derive(Debug, Clone)]
pub struct TConnectionTx<M> {
    /// The sender of the events.
    pub event: crossbeam_channel::Sender<Event>,
    /// The sender of the LSP messages.
    pub lsp: crossbeam_channel::Sender<Message>,
    pub(crate) marker: std::marker::PhantomData<M>,
}

/// The sender of the language server.
#[derive(Debug, Clone)]
pub struct TConnectionRx<M> {
    /// The receiver of the events.
    pub event: crossbeam_channel::Receiver<Event>,
    /// The receiver of the LSP messages.
    pub lsp: crossbeam_channel::Receiver<Message>,
    pub(crate) marker: std::marker::PhantomData<M>,
}

impl<M: TryFrom<Message, Error = anyhow::Error>> TConnectionRx<M> {
    /// Receives a message or an event.
    pub(crate) fn recv(&self) -> anyhow::Result<EventOrMessage<M>> {
        crossbeam_channel::select_biased! {
            recv(self.lsp) -> msg => Ok(EventOrMessage::Msg(msg?.try_into()?)),
            recv(self.event) -> event => Ok(event.map(EventOrMessage::Evt)?),
        }
    }
}

/// This is a helper enum to handle both events and messages.
pub(crate) enum EventOrMessage<M> {
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

    pub(crate) msg_kind: MessageKind,
    pub(crate) sender: Weak<ConnectionTx>,
    pub(crate) req_queue: Arc<Mutex<ReqQueue>>,
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

    /// Finally sends the response if it is not sent before.
    /// From the definition, the response is already sent if it is `Some(())`.
    pub(crate) fn schedule_tail(&self, req_id: RequestId, resp: ScheduledResult) {
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

type AsyncHandler<S, T, R> = fn(srv: &mut S, args: T) -> SchedulableResponse<R>;
type RawHandler<S, T> = fn(srv: &mut S, req_id: RequestId, args: T) -> ScheduledResult;
type BoxPureHandler<S, T> = Box<dyn Fn(&mut S, T) -> LspResult<()>>;
type BoxHandler<S, T> = Box<dyn Fn(&mut S, &LspClient, RequestId, T) -> ScheduledResult>;
type ExecuteCmdMap<S> = HashMap<&'static str, BoxHandler<S, Vec<JsonValue>>>;
type RegularCmdMap<S> = HashMap<&'static str, BoxHandler<S, JsonValue>>;
type NotifyCmdMap<S> = HashMap<&'static str, BoxPureHandler<S, JsonValue>>;
type ResourceMap<S> = HashMap<ImmutPath, BoxHandler<S, Vec<JsonValue>>>;
type MayInitBoxHandler<A, S, T> =
    Box<dyn for<'a> Fn(ServiceState<'a, A, S>, &LspClient, T) -> anyhow::Result<()>>;
type EventMap<A, S> = HashMap<core::any::TypeId, MayInitBoxHandler<A, S, Event>>;

/// A trait that initializes the language server.
pub trait Initializer {
    /// The type of the initialization request.
    type I: for<'de> serde::Deserialize<'de>;
    /// The type of the service.
    type S;

    /// Handles the initialization request.
    /// If the behind protocol is the standard LSP, the request is
    /// `InitializeParams`.
    fn initialize(self, req: Self::I) -> (Self::S, AnySchedulableResponse);
}

/// The language server builder serving LSP.
#[cfg(feature = "lsp")]
pub type LspBuilder<Args> = LsBuilder<LspMessage, Args>;
/// The language server builder serving DAP.
#[cfg(feature = "dap")]
pub type DapBuilder<Args> = LsBuilder<DapMessage, Args>;

/// The builder pattern for the language server.
pub struct LsBuilder<M, Args: Initializer> {
    /// The extra initialization arguments.
    pub args: Args,
    /// The client surface for the implementing language server.
    pub client: LspClient,
    /// The event handlers.
    pub events: EventMap<Args, Args::S>,
    /// The command handlers.
    pub command_handlers: ExecuteCmdMap<Args::S>,
    /// The notification handlers.
    pub notif_handlers: NotifyCmdMap<Args::S>,
    /// The LSP request handlers.
    pub req_handlers: RegularCmdMap<Args::S>,
    /// The resource handlers.
    pub resource_handlers: ResourceMap<Args::S>,
    _marker: std::marker::PhantomData<M>,
}

impl<M, Args: Initializer> LsBuilder<M, Args>
where
    Args::S: 'static,
{
    /// Creates a new language server builder.
    pub fn new(args: Args, client: LspClient) -> Self {
        Self {
            args,
            client,
            events: EventMap::new(),
            command_handlers: ExecuteCmdMap::new(),
            notif_handlers: NotifyCmdMap::new(),
            req_handlers: RegularCmdMap::new(),
            resource_handlers: ResourceMap::new(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Registers an event handler.
    pub fn with_event<T: std::any::Any>(
        mut self,
        ins: &T,
        handler: impl for<'a> Fn(ServiceState<'a, Args, Args::S>, T) -> anyhow::Result<()> + 'static,
    ) -> Self {
        self.events.insert(
            ins.type_id(),
            Box::new(move |s, _client, req| handler(s, *req.downcast().unwrap())),
        );
        self
    }

    /// Registers a raw resource handler.
    pub fn with_resource_(
        mut self,
        path: ImmutPath,
        handler: RawHandler<Args::S, Vec<JsonValue>>,
    ) -> Self {
        self.resource_handlers.insert(path, raw_to_boxed(handler));
        self
    }

    /// Registers an async resource handler.
    pub fn with_resource(
        mut self,
        path: &'static str,
        handler: fn(&mut Args::S, Vec<JsonValue>) -> AnySchedulableResponse,
    ) -> Self {
        self.resource_handlers.insert(
            Path::new(path).into(),
            Box::new(move |s, client, req_id, req| client.schedule(req_id, handler(s, req))),
        );
        self
    }

    /// Builds the language server driver.
    pub fn build(self) -> LsDriver<M, Args> {
        LsDriver {
            state: State::Uninitialized(Some(Box::new(self.args))),
            events: self.events,
            client: self.client,
            commands: self.command_handlers,
            notifications: self.notif_handlers,
            requests: self.req_handlers,
            resources: self.resource_handlers,
            _marker: std::marker::PhantomData,
        }
    }
}

/// An enum to represent the state of the language server.
pub enum ServiceState<'a, A, S> {
    /// The service is uninitialized.
    Uninitialized(Option<&'a mut A>),
    /// The service is initializing.
    Ready(&'a mut S),
}

impl<A, S> ServiceState<'_, A, S> {
    /// Converts the state to an option holding the ready service.
    pub fn ready(&mut self) -> Option<&mut S> {
        match self {
            ServiceState::Ready(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
enum State<Args, S> {
    Uninitialized(Option<Box<Args>>),
    Initializing(S),
    Ready(S),
    ShuttingDown,
}

impl<Args, S> State<Args, S> {
    fn opt(&self) -> Option<&S> {
        match &self {
            State::Ready(s) => Some(s),
            _ => None,
        }
    }

    fn opt_mut(&mut self) -> Option<&mut S> {
        match self {
            State::Ready(s) => Some(s),
            _ => None,
        }
    }
}

/// The language server driver.
pub struct LsDriver<M, Args: Initializer> {
    /// State to synchronize with the client.
    state: State<Args, Args::S>,
    /// The language server client.
    pub client: LspClient,

    // Handle maps
    /// Events for dispatching.
    pub events: EventMap<Args, Args::S>,
    /// Extra commands provided with `textDocument/executeCommand`.
    pub commands: ExecuteCmdMap<Args::S>,
    /// Notifications for dispatching.
    pub notifications: NotifyCmdMap<Args::S>,
    /// Requests for dispatching.
    pub requests: RegularCmdMap<Args::S>,
    /// Resources for dispatching.
    pub resources: ResourceMap<Args::S>,
    _marker: std::marker::PhantomData<M>,
}

impl<M, Args: Initializer> LsDriver<M, Args> {
    /// Gets the state of the language server.
    pub fn state(&self) -> Option<&Args::S> {
        self.state.opt()
    }

    /// Gets the mutable state of the language server.
    pub fn state_mut(&mut self) -> Option<&mut Args::S> {
        self.state.opt_mut()
    }

    /// Makes the language server ready.
    pub fn ready(&mut self, params: Args::I) -> AnySchedulableResponse {
        let args = match &mut self.state {
            State::Uninitialized(args) => args,
            _ => return just_result(Err(invalid_request("server is already initialized"))),
        };

        let args = args.take().expect("already initialized");
        let (s, res) = args.initialize(params);
        self.state = State::Ready(s);

        res
    }

    /// Get static resources with help of tinymist service, for example, a
    /// static help pages for some typst function.
    pub fn get_resources(&mut self, req_id: RequestId, args: Vec<JsonValue>) -> ScheduledResult {
        let s = self.state.opt_mut().ok_or_else(not_initialized)?;

        let path =
            from_value::<PathBuf>(args[0].clone()).map_err(|e| invalid_params(e.to_string()))?;

        let Some(handler) = self.resources.get(path.as_path()) else {
            log::error!("asked for unknown resource: {path:?}");
            return Err(method_not_found());
        };

        // Note our redirection will keep the first path argument in the args vec.
        handler(s, &self.client, req_id, args)
    }
}

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

/// Creates an invalid params error.
pub fn invalid_params(msg: impl fmt::Display) -> ResponseError {
    resp_err(ErrorCode::InvalidParams, msg)
}

/// Creates an internal error.
pub fn internal_error(msg: impl fmt::Display) -> ResponseError {
    resp_err(ErrorCode::InternalError, msg)
}

/// Creates a not initialized error.
pub fn not_initialized() -> ResponseError {
    resp_err(ErrorCode::ServerNotInitialized, "not initialized yet")
}

/// Creates a method not found error.
pub fn method_not_found() -> ResponseError {
    resp_err(ErrorCode::MethodNotFound, "method not found")
}

/// Creates an invalid request error.
pub fn invalid_request(msg: impl fmt::Display) -> ResponseError {
    resp_err(ErrorCode::InvalidRequest, msg)
}

fn from_json<T: serde::de::DeserializeOwned>(json: JsonValue) -> LspResult<T> {
    serde_json::from_value(json).map_err(invalid_request)
}

fn raw_to_boxed<S: 'static, T: 'static>(handler: RawHandler<S, T>) -> BoxHandler<S, T> {
    Box::new(move |s, _client, req_id, req| handler(s, req_id, req))
}

fn resp_err(code: ErrorCode, msg: impl fmt::Display) -> ResponseError {
    ResponseError {
        code: code as i32,
        message: msg.to_string(),
        data: None,
    }
}
