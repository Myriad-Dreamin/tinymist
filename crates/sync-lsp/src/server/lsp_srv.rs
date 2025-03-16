use super::*;

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

        let Some(sender) = self.sender.upgrade() else {
            log::warn!("failed to send request: connection closed");
            return;
        };
        if let Err(res) = sender.lsp.send(request.into()) {
            log::warn!("failed to send request: {res:?}");
        }
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
        let method = &notif.method;
        let Some(sender) = self.sender.upgrade() else {
            log::warn!("failed to send notification ({method}): connection closed");
            return;
        };
        if let Err(res) = sender.lsp.send(notif.into()) {
            log::warn!("failed to send notification: {res:?}");
        }
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
    pub fn start(
        &mut self,
        inbox: TConnectionRx<LspMessage>,
        is_replay: bool,
    ) -> anyhow::Result<()> {
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
                Msg(LspMessage::Request(req)) => self.on_lsp_request(loop_start, req),
                Msg(LspMessage::Notification(not)) => {
                    let is_exit = not.method == EXIT_METHOD;
                    self.on_notification(loop_start, not)?;
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

    /// Registers and handles a request. This should only be called once per
    /// incoming request.
    fn on_lsp_request(&mut self, request_received: Instant, req: Request) {
        self.client
            .register_request(&req.method, &req.id, request_received);

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
            (State::Ready(s), _) => 'serve_req: {
                let method = req.method.as_str();
                let is_shutdown = method == request::Shutdown::METHOD;

                let Some(handler) = self.requests.get(method) else {
                    log::warn!("unhandled lsp request: {method}");
                    break 'serve_req just_result(Err(method_not_found()));
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

    /// Handles an incoming notification.
    fn on_notification(&mut self, received_at: Instant, not: Notification) -> anyhow::Result<()> {
        self.client.start_notification(&not.method);
        let handle = |s, Notification { method, params }: Notification| {
            let Some(handler) = self.notifications.get(method.as_str()) else {
                log::warn!("unhandled notification: {method}");
                return Ok(());
            };

            let result = handler(s, params);
            self.client.stop_notification(&method, received_at, result);

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
