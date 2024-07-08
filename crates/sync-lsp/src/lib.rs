use std::any::Any;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};

use futures::future::MaybeDone;
use lsp_server::{ErrorCode, Message, Notification, Request, RequestId, Response, ResponseError};
use lsp_types::notification::{Notification as Notif, PublishDiagnostics};
use lsp_types::request::{self, RegisterCapability, Request as Req, UnregisterCapability};
use lsp_types::*;
use parking_lot::{Mutex, RwLock};
use reflexo::{time::Instant, ImmutPath};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{from_value, Value as JsonValue};
use tinymist_query::CompilerQueryResponse;

pub mod req_queue;
pub mod transport;

pub type ReqHandler<S> = Box<dyn for<'a> FnOnce(&'a mut S, lsp_server::Response) + Send + Sync>;
type ReqQueue<S> = req_queue::ReqQueue<(String, Instant), ReqHandler<S>>;

pub type LspResult<Res> = Result<Res, ResponseError>;

/// Returns Ok(Some()) -> Already responded
/// Returns Ok(None) -> Need to respond none
/// Returns Err(..) -> Need to respond error
pub type ScheduledResult = LspResult<Option<()>>;

pub type ResponseFuture<T> = MaybeDone<Pin<Box<dyn std::future::Future<Output = T> + Send>>>;
pub type LspResponseFuture<T> = LspResult<ResponseFuture<T>>;
pub type QueryFuture = anyhow::Result<ResponseFuture<anyhow::Result<CompilerQueryResponse>>>;

pub type SchedulableResponse<T> = LspResponseFuture<LspResult<T>>;
pub type AnySchedulableResponse = SchedulableResponse<JsonValue>;

#[macro_export]
macro_rules! just_ok {
    ($expr:expr) => {
        Ok(futures::future::MaybeDone::Done(Ok($expr)))
    };
}

#[macro_export]
macro_rules! just_result {
    ($expr:expr) => {
        Ok(futures::future::MaybeDone::Done($expr))
    };
}

#[macro_export]
macro_rules! just_future {
    ($expr:expr) => {
        Ok(futures::future::MaybeDone::Future(Box::pin($expr)))
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

type AnyCaster<S> = Arc<dyn Fn(&mut dyn Any) -> &mut S + Send + Sync>;

pub struct TypedLspClient<S> {
    client: LspClient,
    caster: AnyCaster<S>,
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

impl<S> TypedLspClient<S> {
    pub fn to_untyped(self) -> LspClient {
        self.client
    }
}

impl<S: 'static> TypedLspClient<S> {
    pub fn cast<T: 'static>(&self, f: fn(&mut S) -> &mut T) -> TypedLspClient<T> {
        let caster = self.caster.clone();
        TypedLspClient {
            client: self.client.clone(),
            caster: Arc::new(move |s| f(caster(s))),
        }
    }

    pub fn send_request<R: lsp_types::request::Request>(
        &self,
        params: R::Params,
        handler: impl FnOnce(&mut S, lsp_server::Response) + Send + Sync + 'static,
    ) {
        let caster = self.caster.clone();
        self.client
            .send_request_::<R>(params, move |s, resp| handler(caster(s), resp))
    }
}

/// The host for the language server, or known as the LSP client.
#[derive(Debug)]
pub struct LspClient {
    /// The tokio handle.
    pub handle: tokio::runtime::Handle,

    sender: Arc<RwLock<Option<crossbeam_channel::Sender<Message>>>>,
    req_queue: Arc<Mutex<ReqQueue<dyn Any>>>,
}

impl Clone for LspClient {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
            sender: self.sender.clone(),
            req_queue: self.req_queue.clone(),
        }
    }
}

impl LspClient {
    /// Creates a new language server host.
    pub fn new(
        handle: tokio::runtime::Handle,
        sender: Arc<RwLock<Option<crossbeam_channel::Sender<Message>>>>,
    ) -> Self {
        Self {
            handle,
            sender,
            req_queue: Arc::new(Mutex::new(ReqQueue::default())),
        }
    }

    pub fn to_typed<S: Any>(&self) -> TypedLspClient<S> {
        TypedLspClient {
            client: self.clone(),
            caster: Arc::new(|s| s.downcast_mut().unwrap()),
        }
    }

    fn force_drop(&self) -> ForceDrop<crossbeam_channel::Sender<Message>> {
        ForceDrop(self.sender.clone())
    }

    pub fn has_pending_requests(&self) -> bool {
        self.req_queue.lock().incoming.has_pending()
    }

    pub fn send_request_<R: lsp_types::request::Request>(
        &self,
        params: R::Params,
        handler: impl FnOnce(&mut dyn Any, lsp_server::Response) + Send + Sync + 'static,
    ) {
        let mut req_queue = self.req_queue.lock();
        let sender = self.sender.read();
        let Some(sender) = sender.as_ref() else {
            log::warn!("failed to send request: connection closed");
            return;
        };
        let request = req_queue
            .outgoing
            .register(R::METHOD.to_owned(), params, Box::new(handler));
        let Err(res) = sender.send(request.into()) else {
            return;
        };
        log::warn!("failed to send request: {res:?}");
    }

    pub fn complete_request<S: Any>(&self, service: &mut S, response: lsp_server::Response) {
        let mut req_queue = self.req_queue.lock();
        let Some(handler) = req_queue.outgoing.complete(response.id.clone()) else {
            log::warn!("received response for unknown request");
            return;
        };
        drop(req_queue);
        handler(service, response)
    }

    pub fn send_notification_(&self, notif: lsp_server::Notification) {
        let sender = self.sender.read();
        let Some(sender) = sender.as_ref() else {
            log::warn!("failed to send notification: connection closed");
            return;
        };
        let Err(res) = sender.send(notif.into()) else {
            return;
        };
        log::warn!("failed to send notification: {res:?}");
    }

    pub fn send_notification<N: lsp_types::notification::Notification>(&self, params: N::Params) {
        self.send_notification_(lsp_server::Notification::new(N::METHOD.to_owned(), params));
    }

    pub fn register_request(&self, request: &lsp_server::Request, request_received: Instant) {
        let mut req_queue = self.req_queue.lock();
        log::info!(
            "handling {} - ({}) at {:0.2?}",
            request.method,
            request.id,
            request_received
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
                log::warn!("failed to send response: connection closed");
                return;
            };

            // if let Some(err) = &response.error {
            //     if err.message.starts_with("server panicked") {
            //         self.poke_rust_analyzer_developer(format!("{}, check the log",
            // err.message))     }
            // }

            let duration = start.elapsed();
            log::info!(
                "handled  {} - ({}) in {:0.2?}",
                method,
                response.id,
                duration
            );
            let Err(res) = sender.send(response.into()) else {
                return;
            };
            log::warn!("failed to send response: {res:?}");
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
        self.send_request_::<RegisterCapability>(
            RegistrationParams { registrations },
            |_, resp| {
                if let Some(err) = resp.error {
                    log::error!("failed to register capability: {err:?}");
                }
            },
        );
        Ok(())
    }

    pub fn unregister_capability(
        &self,
        unregisterations: Vec<Unregistration>,
    ) -> anyhow::Result<()> {
        self.send_request_::<UnregisterCapability>(
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

impl LspClient {
    pub fn schedule_query(&self, req_id: RequestId, query_fut: QueryFuture) -> ScheduledResult {
        let fut = query_fut.map_err(|e| internal_error(e.to_string()))?;
        let fut: AnySchedulableResponse = Ok(match fut {
            MaybeDone::Done(res) => MaybeDone::Done(
                res.and_then(|res| Ok(res.to_untyped()?))
                    .map_err(|err| internal_error(err.to_string())),
            ),
            MaybeDone::Future(fut) => MaybeDone::Future(Box::pin(async move {
                let res = fut.await;
                res.and_then(|res| Ok(res.to_untyped()?))
                    .map_err(|err| internal_error(err.to_string()))
            })),
            MaybeDone::Gone => MaybeDone::Gone,
        });
        self.schedule(req_id, fut)
    }

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

    fn schedule_tail(&self, req_id: RequestId, resp: ScheduledResult) {
        match resp {
            Ok(Some(())) => {}
            _ => self.respond(result_to_response(req_id, resp)),
        }
    }
}

type LspRawPureHandler<S, T> = fn(srv: &mut S, args: T) -> LspResult<()>;
type LspRawHandler<S, T> = fn(srv: &mut S, req_id: RequestId, args: T) -> ScheduledResult;
type LspBoxPureHandler<S, T> = Box<dyn Fn(&mut S, T) -> LspResult<()>>;
type LspBoxHandler<S, T> = Box<dyn Fn(&mut S, &LspClient, RequestId, T) -> ScheduledResult>;
type ExecuteCmdMap<S> = HashMap<&'static str, LspBoxHandler<S, Vec<JsonValue>>>;
type RegularCmdMap<S> = HashMap<&'static str, LspBoxHandler<S, JsonValue>>;
type NotifyCmdMap<S> = HashMap<&'static str, LspBoxPureHandler<S, JsonValue>>;
type ResourceMap<S> = HashMap<ImmutPath, LspBoxHandler<S, Vec<JsonValue>>>;

pub trait Initializer {
    type I: for<'de> serde::Deserialize<'de>;
    type S;

    fn initialize(self, req: Self::I) -> (Self::S, AnySchedulableResponse);
}

pub struct LspBuilder<Args: Initializer> {
    pub args: Args,
    pub client: LspClient,
    pub exec_cmds: ExecuteCmdMap<Args::S>,
    pub notify_cmds: NotifyCmdMap<Args::S>,
    pub regular_cmds: RegularCmdMap<Args::S>,
    pub resource_routes: ResourceMap<Args::S>,
}

impl<Args: Initializer> LspBuilder<Args>
where
    Args::S: 'static,
{
    pub fn new(args: Args, client: LspClient) -> Self {
        Self {
            args,
            client,
            exec_cmds: ExecuteCmdMap::new(),
            notify_cmds: NotifyCmdMap::new(),
            regular_cmds: RegularCmdMap::new(),
            resource_routes: ResourceMap::new(),
        }
    }

    pub fn with_command_(
        mut self,
        cmd: &'static str,
        handler: LspRawHandler<Args::S, Vec<JsonValue>>,
    ) -> Self {
        self.exec_cmds.insert(
            cmd,
            Box::new(move |s, _client, req_id, req| handler(s, req_id, req)),
        );
        self
    }

    pub fn with_command(
        mut self,
        cmd: &'static str,
        handler: fn(&mut Args::S, Vec<JsonValue>) -> AnySchedulableResponse,
    ) -> Self {
        self.exec_cmds.insert(
            cmd,
            Box::new(move |s, client, req_id, req| client.schedule(req_id, handler(s, req))),
        );
        self
    }

    pub fn with_notification_<R: Notif>(
        mut self,
        handler: LspRawPureHandler<Args::S, JsonValue>,
    ) -> Self {
        self.notify_cmds.insert(R::METHOD, Box::new(handler));
        self
    }

    pub fn with_notification<R: Notif>(
        mut self,
        handler: LspRawPureHandler<Args::S, R::Params>,
    ) -> Self {
        self.notify_cmds.insert(
            R::METHOD,
            Box::new(move |s, req| {
                let req = serde_json::from_value::<R::Params>(req).unwrap(); // todo: soft unwrap
                handler(s, req)
            }),
        );
        self
    }

    pub fn with_raw_request<R: Req>(mut self, handler: LspRawHandler<Args::S, JsonValue>) -> Self {
        self.regular_cmds.insert(
            R::METHOD,
            Box::new(move |s, _client, req_id, req| handler(s, req_id, req)),
        );
        self
    }

    // todo: unsafe typed
    pub fn with_request_<R: Req>(
        mut self,
        handler: fn(&mut Args::S, RequestId, R::Params) -> ScheduledResult,
    ) -> Self {
        self.regular_cmds.insert(
            R::METHOD,
            Box::new(move |s, _client, req_id, req| {
                let req = serde_json::from_value::<R::Params>(req).unwrap(); // todo: soft unwrap
                handler(s, req_id, req)
            }),
        );
        self
    }

    pub fn with_request<R: Req>(
        mut self,
        handler: fn(&mut Args::S, R::Params) -> SchedulableResponse<R::Result>,
    ) -> Self {
        self.regular_cmds.insert(
            R::METHOD,
            Box::new(move |s, client, req_id, req| {
                let req = serde_json::from_value::<R::Params>(req).unwrap(); // todo: soft unwrap
                let res = handler(s, req);
                client.schedule(req_id, res)
            }),
        );
        self
    }

    pub fn with_resource_(
        mut self,
        path: ImmutPath,
        handler: LspRawHandler<Args::S, Vec<JsonValue>>,
    ) -> Self {
        self.resource_routes.insert(
            path,
            Box::new(move |s, _client, req_id, req| handler(s, req_id, req)),
        );
        self
    }

    pub fn with_resource(
        mut self,
        path: &'static str,
        handler: fn(&mut Args::S, Vec<JsonValue>) -> AnySchedulableResponse,
    ) -> Self {
        self.resource_routes.insert(
            Path::new(path).into(),
            Box::new(move |s, client, req_id, req| client.schedule(req_id, handler(s, req))),
        );
        self
    }

    pub fn build(self) -> LspDriver<Args> {
        LspDriver {
            state: State::Uninitialized(Some(Box::new(self.args))),
            client: self.client,
            commands: self.exec_cmds,
            notifications: self.notify_cmds,
            requests: self.regular_cmds,
            resources: self.resource_routes,
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

pub struct LspDriver<Args: Initializer> {
    /// State to synchronize with the client.
    state: State<Args, Args::S>,
    /// The language server client.
    pub client: LspClient,

    // Handle maps
    /// Extra commands provided with `textDocument/executeCommand`.
    pub commands: ExecuteCmdMap<Args::S>,
    /// Notifications for dispatching.
    pub notifications: NotifyCmdMap<Args::S>,
    /// Requests for dispatching.
    pub requests: RegularCmdMap<Args::S>,
    /// Resource for dispatching.
    pub resources: ResourceMap<Args::S>,
}

impl<Args: Initializer> LspDriver<Args> {
    pub fn state(&self) -> Option<&Args::S> {
        match &self.state {
            State::Ready(s) => Some(s),
            _ => None,
        }
    }

    pub fn state_mut(&mut self) -> Option<&mut Args::S> {
        match &mut self.state {
            State::Ready(s) => Some(s),
            _ => None,
        }
    }

    pub fn ready(&mut self, params: Args::I) -> AnySchedulableResponse {
        let args = match &mut self.state {
            State::Uninitialized(args) => args,
            _ => {
                return just_result!(Err(resp_err(
                    ErrorCode::InvalidRequest,
                    "Server is already initialized"
                )))
            }
        };

        let (s, res) = args.take().unwrap().initialize(params);
        self.state = State::Ready(s);

        res
    }
}

impl<Args: Initializer> LspDriver<Args>
where
    Args::S: 'static,
{
    pub fn start(
        &mut self,
        inbox: crossbeam_channel::Receiver<Message>,
        is_replay: bool,
    ) -> anyhow::Result<()> {
        let _drop_guard = self.client.force_drop();
        let res = self.start_(inbox);

        if is_replay {
            let client = self.client.clone();
            let _ = std::thread::spawn(move || {
                client.handle.block_on(async {
                    while client.has_pending_requests() {
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                })
            })
            .join();
        }

        res
    }

    pub fn start_(&mut self, inbox: crossbeam_channel::Receiver<Message>) -> anyhow::Result<()> {
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
            const EXIT_METHOD: &str = lsp_types::notification::Exit::METHOD;
            let loop_start = Instant::now();
            match msg {
                Message::Request(req) => self.on_request(loop_start, req),
                Message::Notification(not) => {
                    let is_exit = not.method == EXIT_METHOD;
                    self.on_notification(loop_start, not)?;
                    if is_exit {
                        return Ok(());
                    }
                }
                Message::Response(resp) => {
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
                let params = serde_json::from_value::<Args::I>(req.params).unwrap();
                let (s, res) = args.take().unwrap().initialize(params);
                self.state = State::Initializing(s);
                res
            }
            (State::Uninitialized(..) | State::Initializing(..), _) => {
                just_result!(Err(resp_err(
                    ErrorCode::ServerNotInitialized,
                    "Server is not initialized yet",
                )))
            }
            (_, request::Initialize::METHOD) => just_result!(Err(resp_err(
                ErrorCode::InvalidRequest,
                "Server is already initialized",
            ))),
            // todo: generalize this
            (State::Ready(..), request::ExecuteCommand::METHOD) => {
                reschedule!(self.on_execute_command(req))
            }
            (State::Ready(s), _) => {
                let is_shutdown = req.method == request::Shutdown::METHOD;

                let Some(handler) = self.requests.get(req.method.as_str()) else {
                    log::warn!("unhandled request: {}", req.method);
                    return;
                };

                let result = handler(s, &self.client, req.id.clone(), req.params);
                self.client.schedule_tail(req.id, result);

                if is_shutdown {
                    self.state = State::ShuttingDown;
                }

                return;
            }
            (State::ShuttingDown, _) => just_result!(Err(resp_err(
                ErrorCode::InvalidRequest,
                "Server is shutting down",
            ))),
        };

        let result = self.client.schedule(req_id.clone(), resp);
        self.client.schedule_tail(req_id, result);
    }

    /// The entry point for the `workspace/executeCommand` request.
    fn on_execute_command(&mut self, req: Request) -> ScheduledResult {
        let s = match &mut self.state {
            State::Ready(s) => s,
            _ => {
                return Err(resp_err(
                    ErrorCode::ServerNotInitialized,
                    "Server is not ready",
                ))
            }
        };

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
        let s = match &mut self.state {
            State::Ready(s) => s,
            _ => {
                return Err(resp_err(
                    ErrorCode::ServerNotInitialized,
                    "Server is not ready",
                ))
            }
        };

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
            if let Err(err) = result {
                log::error!(
                    "notifing {} failed in {:0.2?}: {:?}",
                    not.method,
                    request_duration,
                    err
                );
            } else {
                log::info!(
                    "notifing {} succeeded in {:0.2?}",
                    not.method,
                    request_duration
                );
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

struct ForceDrop<T>(Arc<RwLock<Option<T>>>);
impl<T> Drop for ForceDrop<T> {
    fn drop(&mut self) {
        *self.0.write() = None;
    }
}

fn resp_err(code: ErrorCode, msg: impl Into<String>) -> ResponseError {
    ResponseError {
        code: code as i32,
        message: msg.into(),
        data: None,
    }
}

pub fn from_json<T: DeserializeOwned>(
    what: &'static str,
    json: &serde_json::Value,
) -> anyhow::Result<T> {
    serde_json::from_value(json.clone())
        .map_err(|e| anyhow::anyhow!("Failed to deserialize {what}: {e}; {json}"))
}

pub fn invalid_params(msg: impl Into<String>) -> ResponseError {
    ResponseError {
        code: ErrorCode::InvalidParams as i32,
        message: msg.into(),
        data: None,
    }
}

pub fn internal_error(msg: impl Into<String>) -> ResponseError {
    ResponseError {
        code: ErrorCode::InternalError as i32,
        message: msg.into(),
        data: None,
    }
}

pub fn method_not_found() -> ResponseError {
    ResponseError {
        code: ErrorCode::MethodNotFound as i32,
        message: "Method not found".to_string(),
        data: None,
    }
}

pub fn result_to_response<T: Serialize>(
    id: RequestId,
    result: Result<T, ResponseError>,
) -> Response {
    match result {
        Ok(resp) => match serde_json::to_value(resp) {
            Ok(resp) => Response::new_ok(id, resp),
            Err(e) => {
                let e = internal_error(e.to_string());
                Response::new_err(id, e.code, e.message)
            }
        },
        Err(e) => Response::new_err(id, e.code, e.message),
    }
}
