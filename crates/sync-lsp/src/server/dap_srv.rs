use dapts::IRequest;

use super::*;

impl LspClient {
    /// Sends a dap event to the client.
    pub fn send_dap_event<E: dapts::IEvent>(&self, body: E::Body) {
        let req_id = self.req_queue.lock().outgoing.alloc_request_id();

        self.send_dap_event_(dap::Event::new(req_id as i64, E::EVENT.to_owned(), body));
    }

    /// Sends an untyped dap_event to the client.
    pub fn send_dap_event_(&self, evt: dap::Event) {
        let method = &evt.event;
        let Some(sender) = self.sender.upgrade() else {
            log::warn!("failed to send dap event ({method}): connection closed");
            return;
        };
        if let Err(res) = sender.lsp.send(evt.into()) {
            log::warn!("failed to send dap event: {res:?}");
        }
    }
}

impl<Args: Initializer> LsBuilder<DapMessage, Args>
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

    /// Registers a raw request handler that handlers a kind of untyped lsp
    /// request.
    pub fn with_raw_request<R: dapts::IRequest>(
        mut self,
        handler: RawHandler<Args::S, JsonValue>,
    ) -> Self {
        self.req_handlers.insert(R::COMMAND, raw_to_boxed(handler));
        self
    }

    // todo: unsafe typed
    /// Registers an raw request handler that handlers a kind of typed lsp
    /// request.
    pub fn with_request_<R: dapts::IRequest>(
        mut self,
        handler: fn(&mut Args::S, RequestId, R::Arguments) -> ScheduledResult,
    ) -> Self {
        self.req_handlers.insert(
            R::COMMAND,
            Box::new(move |s, _client, req_id, req| handler(s, req_id, from_json(req)?)),
        );
        self
    }

    /// Registers a typed request handler.
    pub fn with_request<R: dapts::IRequest>(
        mut self,
        handler: AsyncHandler<Args::S, R::Arguments, R::Response>,
    ) -> Self {
        self.req_handlers.insert(
            R::COMMAND,
            Box::new(move |s, client, req_id, req| {
                client.schedule(req_id, handler(s, from_json(req)?))
            }),
        );
        self
    }
}

impl<Args: Initializer> LsDriver<DapMessage, Args>
where
    Args::S: 'static,
{
    /// Starts the debug adaptor on the given connection.
    ///
    /// If `is_replay` is true, the server will wait for all pending requests to
    /// finish before exiting. This is useful for testing the language server.
    ///
    /// See [`transport::MirrorArgs`] for information about the record-replay
    /// feature.
    pub fn start(
        &mut self,
        inbox: TConnectionRx<DapMessage>,
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

    /// Starts the debug adaptor on the given connection.
    pub fn start_(&mut self, inbox: TConnectionRx<DapMessage>) -> anyhow::Result<()> {
        use EventOrMessage::*;

        while let Ok(msg) = inbox.recv() {
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
                Msg(DapMessage::Request(req)) => self.on_request(loop_start, req),
                Msg(DapMessage::Event(not)) => {
                    self.on_event(loop_start, not)?;
                }
                Msg(DapMessage::Response(resp)) => {
                    let s = match &mut self.state {
                        State::Ready(s) => s,
                        _ => {
                            log::warn!("server is not ready yet");
                            continue;
                        }
                    };

                    self.client.clone().complete_dap_request(s, resp)
                }
            }
        }

        log::warn!("client exited without proper shutdown sequence");
        Ok(())
    }

    /// Registers and handles a request. This should only be called once per
    /// incoming request.
    fn on_request(&mut self, request_received: Instant, req: dap::Request) {
        let req_id = (req.seq as i32).into();
        self.client
            .register_request(&req.command, &req_id, request_received);

        let resp = match (&mut self.state, &*req.command) {
            (State::Uninitialized(args), dapts::request::Initialize::COMMAND) => {
                // todo: what will happen if the request cannot be deserialized?
                let params = serde_json::from_value::<Args::I>(req.arguments);
                match params {
                    Ok(params) => {
                        let args = args.take().expect("already initialized");
                        let (s, res) = args.initialize(params);
                        self.state = State::Ready(s);
                        res
                    }
                    Err(e) => just_result(Err(invalid_request(e))),
                }
            }
            // (state, dap::events::Initialized::METHOD) => {
            //     let mut s = State::ShuttingDown;
            //     std::mem::swap(state, &mut s);
            //     match s {
            //         State::Initializing(s) => {
            //             *state = State::Ready(s);
            //         }
            //         _ => {
            //             std::mem::swap(state, &mut s);
            //         }
            //     }

            //     let s = match state {
            //         State::Ready(s) => s,
            //         _ => {
            //             log::warn!("server is not ready yet");
            //             return Ok(());
            //         }
            //     };
            //     handle(s, not)
            // }
            (State::Uninitialized(..) | State::Initializing(..), _) => {
                just_result(Err(not_initialized()))
            }
            (_, dapts::request::Initialize::COMMAND) => {
                just_result(Err(invalid_request("server is already initialized")))
            }
            // todo: generalize this
            // (State::Ready(..), request::ExecuteCommand::METHOD) => {
            // reschedule!(self.on_execute_command(req))
            // }
            (State::Ready(s), _) => 'serve_req: {
                let method = req.command.as_str();

                let is_disconnect = method == dapts::request::Disconnect::COMMAND;

                let Some(handler) = self.requests.get(method) else {
                    log::warn!("unhandled dap request: {method}");
                    break 'serve_req just_result(Err(method_not_found()));
                };

                let result = handler(s, &self.client, req_id.clone(), req.arguments);
                self.client.schedule_tail(req_id, result);

                if is_disconnect {
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

    /// Handles an incoming event.
    fn on_event(&mut self, received_at: Instant, not: dap::Event) -> anyhow::Result<()> {
        self.client.start_notification(&not.event);
        let handle = |s,
                      dap::Event {
                          seq: _,
                          event,
                          body,
                      }: dap::Event| {
            let Some(handler) = self.notifications.get(event.as_str()) else {
                log::warn!("unhandled event: {event}");
                return Ok(());
            };

            let result = handler(s, body);
            self.client.stop_notification(&event, received_at, result);

            Ok(())
        };

        match (&mut self.state, &*not.event) {
            (State::Ready(state), _) => handle(state, not),
            // todo: whether it is safe to ignore events
            (State::Uninitialized(..) | State::Initializing(..), method) => {
                log::warn!("server is not ready yet, while received event {method}");
                Ok(())
            }
            (State::ShuttingDown, method) => {
                log::warn!("server is shutting down, while received event {method}");
                Ok(())
            }
        }
    }
}
