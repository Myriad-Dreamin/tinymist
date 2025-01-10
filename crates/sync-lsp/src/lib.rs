//! A synchronous language server implementation.

use core::fmt;
use std::any::Any;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Weak};
use std::{collections::HashMap, path::PathBuf};

use crossbeam_channel::RecvError;
use futures::future::MaybeDone;
use lsp_server::{ErrorCode, Notification, Request, RequestId, Response};
use lsp_types::{notification::Notification as Notif, request::Request as Req, *};
use parking_lot::Mutex;
use reflexo::{time::Instant, ImmutPath};
use serde::Serialize;
use serde_json::{from_value, Value as JsonValue};

pub mod req_queue;
pub mod transport;

type Event = Box<dyn Any + Send>;

/// The sender of the language server.
#[derive(Debug, Clone)]
pub struct ConnectionTx {
    /// The sender of the events.
    pub event: crossbeam_channel::Sender<Event>,
    /// The sender of the LSP messages.
    pub lsp: crossbeam_channel::Sender<Message>,
}

/// The sender of the language server.
#[derive(Debug, Clone)]
pub struct ConnectionRx {
    /// The receiver of the events.
    pub event: crossbeam_channel::Receiver<Event>,
    /// The receiver of the LSP messages.
    pub lsp: crossbeam_channel::Receiver<Message>,
}

impl ConnectionRx {
    /// Receives a message or an event.
    fn recv(&self) -> Result<EventOrMessage, RecvError> {
        crossbeam_channel::select_biased! {
            recv(self.lsp) -> msg => msg.map(EventOrMessage::Msg),
            recv(self.event) -> event => event.map(EventOrMessage::Evt),
        }
    }
}

/// This is a helper enum to handle both events and messages.
enum EventOrMessage {
    Evt(Event),
    Msg(Message),
}

/// Connection is just a pair of channels of LSP messages.
pub struct Connection {
    /// The senders of the connection.
    pub sender: ConnectionTx,
    /// The receivers of the connection.
    pub receiver: ConnectionRx,
}

/// The common message type for the language server.
pub type Message = lsp_server::Message;

/// The common error type for the language server.
pub use lsp_server::ResponseError;
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
    /// Casts the service to another type.
    pub fn cast<T: 'static>(&self, f: fn(&mut S) -> &mut T) -> TypedLspClient<T> {
        let caster = self.caster.clone();
        TypedLspClient {
            client: self.client.clone(),
            caster: Arc::new(move |s| f(caster(s))),
        }
    }

    /// Sends a request to the client and registers a handler handled by the
    /// service `S`.
    pub fn send_request<R: Req>(
        &self,
        params: R::Params,
        handler: impl FnOnce(&mut S, lsp_server::Response) + Send + Sync + 'static,
    ) {
        let caster = self.caster.clone();
        self.client
            .send_request_::<R>(params, move |s, resp| handler(caster(s), resp))
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
    pub fn new(handle: tokio::runtime::Handle, sender: ConnectionTx) -> Self {
        let _strong = Arc::new(sender);
        let weak = LspClient {
            handle,
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

type ReqHandler = Box<dyn for<'a> FnOnce(&'a mut dyn Any, lsp_server::Response) + Send + Sync>;
type ReqQueue = req_queue::ReqQueue<(String, Instant), ReqHandler>;

/// The host for the language server, or known as the LSP client.
#[derive(Debug, Clone)]
pub struct LspClient {
    /// The tokio handle.
    pub handle: tokio::runtime::Handle,

    sender: Weak<ConnectionTx>,
    req_queue: Arc<Mutex<ReqQueue>>,
}

impl LspClient {
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

    /// Sends a request to the client and registers a handler.
    pub fn send_request_<R: Req>(
        &self,
        params: R::Params,
        handler: impl FnOnce(&mut dyn Any, lsp_server::Response) + Send + Sync + 'static,
    ) {
        let mut req_queue = self.req_queue.lock();
        let Some(sender) = self.sender.upgrade() else {
            log::warn!("failed to send request: connection closed");
            return;
        };
        let request = req_queue
            .outgoing
            .register(R::METHOD.to_owned(), params, Box::new(handler));
        let Err(res) = sender.lsp.send(request.into()) else {
            return;
        };
        log::warn!("failed to send request: {res:?}");
    }

    /// Completes an server2client request in the request queue.
    pub fn complete_request<S: Any>(&self, service: &mut S, response: lsp_server::Response) {
        let mut req_queue = self.req_queue.lock();
        let Some(handler) = req_queue.outgoing.complete(response.id.clone()) else {
            log::warn!("received response for unknown request");
            return;
        };
        drop(req_queue);
        handler(service, response)
    }

    /// Registers an client2server request in the request queue.
    pub fn register_request(&self, request: &lsp_server::Request, received_at: Instant) {
        let mut req_queue = self.req_queue.lock();
        let method = request.method.clone();
        let req_id = request.id.clone();
        log::info!("handling {method} - ({req_id}) at {received_at:0.2?}");
        req_queue.incoming.register(req_id, (method, received_at));
    }

    /// Completes an client2server request in the request queue.
    pub fn respond(&self, response: lsp_server::Response) {
        let mut req_queue = self.req_queue.lock();
        if let Some((method, start)) = req_queue.incoming.complete(response.id.clone()) {
            let Some(sender) = self.sender.upgrade() else {
                log::warn!("failed to send request: connection closed");
                return;
            };

            let duration = start.elapsed();
            log::info!("handled  {method} - ({}) in {duration:0.2?}", response.id);
            let Err(res) = sender.lsp.send(response.into()) else {
                return;
            };
            log::warn!("failed to send response: {res:?}");
        }
    }

    /// Sends an untyped notification to the client.
    pub fn send_notification_(&self, notif: lsp_server::Notification) {
        let Some(sender) = self.sender.upgrade() else {
            log::warn!("failed to send notification: connection closed");
            return;
        };
        let Err(res) = sender.lsp.send(notif.into()) else {
            return;
        };
        log::warn!("failed to send notification: {res:?}");
    }

    /// Sends a typed notification to the client.
    pub fn send_notification<N: Notif>(&self, params: N::Params) {
        self.send_notification_(lsp_server::Notification::new(N::METHOD.to_owned(), params));
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
                self.respond(result_to_response(req_id, output));
            }
            Future(fut) => {
                let client = self.clone();
                let req_id = req_id.clone();
                self.handle.spawn(async move {
                    client.respond(result_to_response(req_id, fut.await));
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
            _ => self.respond(result_to_response(req_id, resp)),
        }
    }
}

type AsyncHandler<S, T, R> = fn(srv: &mut S, args: T) -> SchedulableResponse<R>;
type PureHandler<S, T> = fn(srv: &mut S, args: T) -> LspResult<()>;
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

/// The builder pattern for the language server.
pub struct LspBuilder<Args: Initializer> {
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
    /// The request handlers.
    pub req_handlers: RegularCmdMap<Args::S>,
    /// The resource handlers.
    pub resource_handlers: ResourceMap<Args::S>,
}

impl<Args: Initializer> LspBuilder<Args>
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

    /// Registers an raw event handler.
    pub fn with_command_(
        mut self,
        cmd: &'static str,
        handler: RawHandler<Args::S, Vec<JsonValue>>,
    ) -> Self {
        self.command_handlers.insert(cmd, raw_to_boxed(handler));
        self
    }

    /// Registers an async command handler.
    pub fn with_command<R: Serialize + 'static>(
        mut self,
        cmd: &'static str,
        handler: AsyncHandler<Args::S, Vec<JsonValue>, R>,
    ) -> Self {
        self.command_handlers.insert(
            cmd,
            Box::new(move |s, client, req_id, req| client.schedule(req_id, handler(s, req))),
        );
        self
    }

    /// Registers an untyped notification handler.
    pub fn with_notification_<R: Notif>(
        mut self,
        handler: PureHandler<Args::S, JsonValue>,
    ) -> Self {
        self.notif_handlers.insert(R::METHOD, Box::new(handler));
        self
    }

    /// Registers a typed notification handler.
    pub fn with_notification<R: Notif>(mut self, handler: PureHandler<Args::S, R::Params>) -> Self {
        self.notif_handlers.insert(
            R::METHOD,
            Box::new(move |s, req| handler(s, from_json(req)?)),
        );
        self
    }

    /// Registers a raw request handler that handlers a kind of untyped lsp
    /// request.
    pub fn with_raw_request<R: Req>(mut self, handler: RawHandler<Args::S, JsonValue>) -> Self {
        self.req_handlers.insert(R::METHOD, raw_to_boxed(handler));
        self
    }

    // todo: unsafe typed
    /// Registers an raw request handler that handlers a kind of typed lsp
    /// request.
    pub fn with_request_<R: Req>(
        mut self,
        handler: fn(&mut Args::S, RequestId, R::Params) -> ScheduledResult,
    ) -> Self {
        self.req_handlers.insert(
            R::METHOD,
            Box::new(move |s, _client, req_id, req| handler(s, req_id, from_json(req)?)),
        );
        self
    }

    /// Registers a typed request handler.
    pub fn with_request<R: Req>(
        mut self,
        handler: AsyncHandler<Args::S, R::Params, R::Result>,
    ) -> Self {
        self.req_handlers.insert(
            R::METHOD,
            Box::new(move |s, client, req_id, req| {
                client.schedule(req_id, handler(s, from_json(req)?))
            }),
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
    pub fn build(self) -> LspDriver<Args> {
        LspDriver {
            state: State::Uninitialized(Some(Box::new(self.args))),
            events: self.events,
            client: self.client,
            commands: self.command_handlers,
            notifications: self.notif_handlers,
            requests: self.req_handlers,
            resources: self.resource_handlers,
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
pub struct LspDriver<Args: Initializer> {
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
}

impl<Args: Initializer> LspDriver<Args> {
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
}

impl<Args: Initializer> LspDriver<Args>
where
    Args::S: 'static,
{
    /// Starts the language server on the given connection.
    ///
    /// If `is_replay` is true, the server will wait for all pending requests to
    /// finish before exiting. This is useful for testing the language server.
    ///
    /// See [`transport::MirrorArgs`] for information about the record-replay
    /// feature.
    pub fn start(&mut self, inbox: ConnectionRx, is_replay: bool) -> anyhow::Result<()> {
        let res = self.start_(inbox);

        if is_replay {
            let client = self.client.clone();
            let _ = std::thread::spawn(move || {
                let since = std::time::Instant::now();
                let timeout = std::env::var("REPLAY_TIMEOUT")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(60);
                client.handle.block_on(async {
                    while client.has_pending_requests() {
                        if since.elapsed().as_secs() > timeout {
                            log::error!("replay timeout reached, {timeout}s");
                            client.begin_panic();
                        }

                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                })
            })
            .join();
        }

        res
    }

    /// Starts the language server on the given connection.
    pub fn start_(&mut self, inbox: ConnectionRx) -> anyhow::Result<()> {
        use EventOrMessage::*;
        // todo: follow what rust analyzer does
        // Windows scheduler implements priority boosts: if thread waits for an
        // event (like a condvar), and event fires, priority of the thread is
        // temporary bumped. This optimization backfires in our case: each time
        // the `main_loop` schedules a task to run on a threadpool, the
        // worker threads gets a higher priority, and (on a machine with
        // fewer cores) displaces the main loop! We work around this by
        // marking the main loop as a higher-priority thread.
        //
        // https://docs.microsoft.com/en-us/windows/win32/procthread/scheduling-priorities
        // https://docs.microsoft.com/en-us/windows/win32/procthread/priority-boosts
        // https://github.com/rust-lang/rust-analyzer/issues/2835
        // #[cfg(windows)]
        // unsafe {
        //     use winapi::um::processthreadsapi::*;
        //     let thread = GetCurrentThread();
        //     let thread_priority_above_normal = 1;
        //     SetThreadPriority(thread, thread_priority_above_normal);
        // }

        while let Ok(msg) = inbox.recv() {
            const EXIT_METHOD: &str = notification::Exit::METHOD;
            let loop_start = Instant::now();
            match msg {
                Evt(event) => {
                    let Some(event_handler) = self.events.get(&event.as_ref().type_id()) else {
                        log::warn!("unhandled event: {:?}", event.as_ref().type_id());
                        continue;
                    };

                    let s = match &mut self.state {
                        State::Uninitialized(u) => ServiceState::Uninitialized(u.as_deref_mut()),
                        State::Initializing(s) | State::Ready(s) => ServiceState::Ready(s),
                        State::ShuttingDown => {
                            log::warn!("server is shutting down");
                            continue;
                        }
                    };

                    event_handler(s, &self.client, event)?;
                }
                Msg(Message::Request(req)) => self.on_request(loop_start, req),
                Msg(Message::Notification(not)) => {
                    let is_exit = not.method == EXIT_METHOD;
                    self.on_notification(loop_start, not)?;
                    if is_exit {
                        return Ok(());
                    }
                }
                Msg(Message::Response(resp)) => {
                    let s = match &mut self.state {
                        State::Ready(s) => s,
                        _ => {
                            log::warn!("server is not ready yet");
                            continue;
                        }
                    };

                    self.client.clone().complete_request(s, resp)
                }
            }
        }

        log::warn!("client exited without proper shutdown sequence");
        Ok(())
    }

    /// Registers and handles a request. This should only be called once per
    /// incoming request.
    fn on_request(&mut self, request_received: Instant, req: Request) {
        self.client.register_request(&req, request_received);

        let req_id = req.id.clone();
        let resp = match (&mut self.state, &*req.method) {
            (State::Uninitialized(args), request::Initialize::METHOD) => {
                // todo: what will happen if the request cannot be deserialized?
                let params = serde_json::from_value::<Args::I>(req.params);
                match params {
                    Ok(params) => {
                        let args = args.take().expect("already initialized");
                        let (s, res) = args.initialize(params);
                        self.state = State::Initializing(s);
                        res
                    }
                    Err(e) => just_result(Err(invalid_request(e))),
                }
            }
            (State::Uninitialized(..) | State::Initializing(..), _) => {
                just_result(Err(not_initialized()))
            }
            (_, request::Initialize::METHOD) => {
                just_result(Err(invalid_request("server is already initialized")))
            }
            // todo: generalize this
            (State::Ready(..), request::ExecuteCommand::METHOD) => {
                reschedule!(self.on_execute_command(req))
            }
            (State::Ready(s), _) => {
                let method = req.method.as_str();
                let is_shutdown = method == request::Shutdown::METHOD;

                let Some(handler) = self.requests.get(method) else {
                    log::warn!("unhandled request: {method}");
                    return;
                };

                let result = handler(s, &self.client, req_id.clone(), req.params);
                self.client.schedule_tail(req_id, result);

                if is_shutdown {
                    self.state = State::ShuttingDown;
                }

                return;
            }
            (State::ShuttingDown, _) => {
                just_result(Err(invalid_request("server is shutting down")))
            }
        };

        let result = self.client.schedule(req_id.clone(), resp);
        self.client.schedule_tail(req_id, result);
    }

    /// The entry point for the `workspace/executeCommand` request.
    fn on_execute_command(&mut self, req: Request) -> ScheduledResult {
        let s = self.state.opt_mut().ok_or_else(not_initialized)?;

        let params = from_value::<ExecuteCommandParams>(req.params)
            .map_err(|e| invalid_params(e.to_string()))?;

        let ExecuteCommandParams {
            command, arguments, ..
        } = params;

        // todo: generalize this
        if command == "tinymist.getResources" {
            self.get_resources(req.id, arguments)
        } else {
            let Some(handler) = self.commands.get(command.as_str()) else {
                log::error!("asked to execute unknown command: {command}");
                return Err(method_not_found());
            };
            handler(s, &self.client, req.id, arguments)
        }
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

    /// Handles an incoming notification.
    fn on_notification(
        &mut self,
        request_received: Instant,
        not: Notification,
    ) -> anyhow::Result<()> {
        log::info!("notifying {} - at {:0.2?}", not.method, request_received);
        let handle = |s, not: Notification| {
            let Some(handler) = self.notifications.get(not.method.as_str()) else {
                log::warn!("unhandled notification: {}", not.method);
                return Ok(());
            };

            let result = handler(s, not.params);

            let request_duration = request_received.elapsed();
            let method = &not.method;
            if let Err(err) = result {
                log::error!("notifing {method} failed in {request_duration:0.2?}: {err:?}");
            } else {
                log::info!("notifing {method} succeeded in {request_duration:0.2?}");
            }

            Ok(())
        };

        match (&mut self.state, &*not.method) {
            (state, notification::Initialized::METHOD) => {
                let mut s = State::ShuttingDown;
                std::mem::swap(state, &mut s);
                match s {
                    State::Initializing(s) => {
                        *state = State::Ready(s);
                    }
                    _ => {
                        std::mem::swap(state, &mut s);
                    }
                }

                let s = match state {
                    State::Ready(s) => s,
                    _ => {
                        log::warn!("server is not ready yet");
                        return Ok(());
                    }
                };
                handle(s, not)
            }
            (State::Ready(state), _) => handle(state, not),
            // todo: whether it is safe to ignore notifications
            (State::Uninitialized(..) | State::Initializing(..), method) => {
                log::warn!("server is not ready yet, while received notification {method}");
                Ok(())
            }
            (State::ShuttingDown, method) => {
                log::warn!("server is shutting down, while received notification {method}");
                Ok(())
            }
        }
    }
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

/// Converts an [`LspResult`] to a response.
pub fn result_to_response<T: Serialize>(id: RequestId, result: LspResult<T>) -> Response {
    match result.and_then(|t| serde_json::to_value(t).map_err(internal_error)) {
        Ok(resp) => Response::new_ok(id, resp),
        Err(e) => Response::new_err(id, e.code, e.message),
    }
}
