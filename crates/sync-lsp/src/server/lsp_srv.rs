use super::*;

use lsp_types::{notification::Notification as Notif, request::Request as Req, *};

type PureHandler<S, T> = fn(srv: &mut S, args: T) -> LspResult<()>;

impl<S: 'static> TypedLspClient<S> {
    /// Sends a request to the client and registers a handler handled by the
    /// service `S`.
    pub fn send_lsp_request<R: Req>(
        &self,
        params: R::Params,
        handler: impl FnOnce(&mut S, lsp::Response) + Send + Sync + 'static,
    ) {
        let caster = self.caster.clone();
        self.client
            .send_lsp_request_::<R>(params, move |s, resp| handler(caster(s), resp))
    }
}

impl LspClient {
    /// Sends a request to the client and registers a handler.
    pub fn send_lsp_request_<R: Req>(
        &self,
        params: R::Params,
        handler: impl FnOnce(&mut dyn Any, lsp::Response) + Send + Sync + 'static,
    ) {
        let mut req_queue = self.req_queue.lock();
        let request = req_queue.outgoing.register(
            R::METHOD.to_owned(),
            params,
            Box::new(|s, resp| handler(s, resp.try_into().unwrap())),
        );

        self.sender.send_message(request.into());
    }

    /// Completes an client2server request in the request queue.
    pub fn respond_lsp(&self, response: lsp::Response) {
        self.respond(response.id.clone(), response.into())
    }

    /// Sends a typed notification to the client.
    pub fn send_notification<N: Notif>(&self, params: &N::Params) {
        self.send_notification_(lsp::Notification::new(N::METHOD.to_owned(), params));
    }

    /// Sends an untyped notification to the client.
    pub fn send_notification_(&self, notif: lsp::Notification) {
        self.sender.send_message(notif.into());
    }
}

impl<Args: Initializer> LsBuilder<LspMessage, Args>
where
    Args::S: 'static,
{
    /// Registers an raw event handler.
    pub fn with_command_(
        mut self,
        cmd: &'static str,
        handler: RawHandler<Args::S, Vec<JsonValue>>,
    ) -> Self {
        self.command_handlers.insert(cmd, Box::new(handler));
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
            Box::new(move |s, req| erased_response(handler(s, req))),
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
        self.req_handlers.insert(R::METHOD, Box::new(handler));
        self
    }

    // todo: unsafe typed
    /// Registers an raw request handler that handlers a kind of typed lsp
    /// request.
    pub fn with_request_<R: Req>(
        mut self,
        handler: fn(&mut Args::S, R::Params) -> ScheduleResult,
    ) -> Self {
        self.req_handlers.insert(
            R::METHOD,
            Box::new(move |s, req| handler(s, from_json(req)?)),
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
            Box::new(move |s, req| erased_response(handler(s, from_json(req)?))),
        );
        self
    }
}

impl<Args: Initializer> LsDriver<LspMessage, Args>
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
    #[cfg(feature = "system")]
    pub fn start(
        &mut self,
        inbox: TConnectionRx<LspMessage>,
        is_replay: bool,
    ) -> anyhow::Result<()> {
        let res = self.start_(inbox);

        if is_replay {
            let client = self.client.clone();
            let _ = std::thread::spawn(move || {
                let since = tinymist_std::time::Instant::now();
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
    #[cfg(feature = "system")]
    pub fn start_(&mut self, inbox: TConnectionRx<LspMessage>) -> anyhow::Result<()> {
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
                Msg(LspMessage::Request(req)) => {
                    let client = self.client.clone();
                    let req_id = req.id.clone();
                    client.register_request(&req.method, &req_id, loop_start);
                    let fut =
                        client.schedule_tail(req_id, self.on_lsp_request(&req.method, req.params));
                    self.client.handle.spawn(fut);
                }
                Msg(LspMessage::Notification(not)) => {
                    let is_exit = not.method == EXIT_METHOD;
                    self.client.hook.start_notification(&not.method);
                    let result = self.on_notification(&not.method, not.params);
                    self.client
                        .hook
                        .stop_notification(&not.method, loop_start, result);
                    if is_exit {
                        return Ok(());
                    }
                }
                Msg(LspMessage::Response(resp)) => {
                    let s = match &mut self.state {
                        State::Ready(s) => s,
                        _ => {
                            log::warn!("server is not ready yet");
                            continue;
                        }
                    };

                    self.client.clone().complete_lsp_request(s, resp)
                }
            }
        }

        log::warn!("client exited without proper shutdown sequence");
        Ok(())
    }

    /// Handles an incoming server event.
    #[cfg(feature = "web")]
    pub fn on_server_event(&mut self, event_id: u32) {
        let evt = match &self.client.sender {
            TransportHost::Js { events, .. } => events.lock().remove(&event_id),
            TransportHost::System(_) => {
                panic!("cannot send server event in system transport");
            }
        };

        if let Some(event) = evt {
            let Some(event_handler) = self.events.get(&event.as_ref().type_id()) else {
                log::warn!("unhandled event: {:?}", event.as_ref().type_id());
                return;
            };

            let s = match &mut self.state {
                State::Uninitialized(u) => ServiceState::Uninitialized(u.as_deref_mut()),
                State::Initializing(s) | State::Ready(s) => ServiceState::Ready(s),
                State::ShuttingDown => {
                    log::warn!("server is shutting down");
                    return;
                }
            };

            let res = event_handler(s, &self.client, event);
            if let Err(err) = res {
                log::error!("failed to handle server event {event_id}: {err}");
            }
        }
    }

    /// Registers and handles a request. This should only be called once per
    /// incoming request.
    pub fn on_lsp_request(&mut self, method: &str, params: JsonValue) -> ScheduleResult {
        match (&mut self.state, method) {
            (State::Uninitialized(args), request::Initialize::METHOD) => {
                // todo: what will happen if the request cannot be deserialized?
                let params = serde_json::from_value::<Args::I>(params);
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
            (State::Ready(..), request::ExecuteCommand::METHOD) => self.on_execute_command(params),
            (State::Ready(s), method) => 'serve_req: {
                let is_shutdown = method == request::Shutdown::METHOD;

                let Some(handler) = self.requests.get(method) else {
                    log::warn!("unhandled lsp request: {method}");
                    break 'serve_req just_result(Err(method_not_found()));
                };

                let resp = handler(s, params);

                if is_shutdown {
                    self.state = State::ShuttingDown;
                }

                resp
            }
            (State::ShuttingDown, _) => {
                just_result(Err(invalid_request("server is shutting down")))
            }
        }
    }

    /// The entry point for the `workspace/executeCommand` request.
    fn on_execute_command(&mut self, params: JsonValue) -> ScheduleResult {
        let s = self.state.opt_mut().ok_or_else(not_initialized)?;

        let params = from_value::<ExecuteCommandParams>(params)
            .map_err(|e| invalid_params(e.to_string()))?;

        let ExecuteCommandParams {
            command, arguments, ..
        } = params;

        // todo: generalize this
        if command == "tinymist.getResources" {
            self.get_resources(arguments)
        } else {
            let Some(handler) = self.commands.get(command.as_str()) else {
                log::error!("asked to execute unknown command: {command}");
                return Err(method_not_found());
            };
            handler(s, arguments)
        }
    }

    /// Handles an incoming notification.
    pub fn on_notification(&mut self, method: &str, params: JsonValue) -> LspResult<()> {
        let handle = |s, method: &str, params: JsonValue| {
            let Some(handler) = self.notifications.get(method) else {
                log::warn!("unhandled notification: {method}");
                return Ok(());
            };

            handler(s, params)
        };

        match (&mut self.state, method) {
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
                handle(s, method, params)
            }
            (State::Ready(state), method) => handle(state, method, params),
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

    /// Handles an incoming response.
    pub fn on_lsp_response(&mut self, resp: lsp::Response) {
        let client = self.client.clone();
        let Some(s) = self.state_mut() else {
            log::warn!("server is not ready yet, while received response");
            return;
        };

        client.complete_lsp_request(s, resp)
    }
}
