//! A synchronous language server implementation.

use core::fmt;
use std::any::Any;
use std::path::Path;
use std::{collections::HashMap, path::PathBuf};

use reflexo::{time::Instant, ImmutPath};
use serde::Serialize;
use serde_json::{from_value, Value as JsonValue};

#[cfg(feature = "dap")]
mod dap_srv;

#[cfg(feature = "lsp")]
mod lsp_srv;
#[cfg(feature = "lsp")]
use lsp::{Notification, Request};

use crate::msg::*;
use crate::*;

type Event = Box<dyn Any + Send>;

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

/// The language server builder serving LSP.
#[cfg(feature = "lsp")]
pub type LspBuilder<Args> = LsBuilder<LspMessage, Args>;
/// The language server builder serving DAP.
#[cfg(feature = "dap")]
pub type DapBuilder<Args> = LsBuilder<DapMessage, Args>;

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
