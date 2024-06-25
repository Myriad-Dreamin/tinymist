use core::fmt;
use std::{collections::HashMap, path::Path, sync::Arc, time::Instant};

use crossbeam_channel::{select, Receiver};
use log::{error, info, warn};
use lsp_server::{ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::Notification as _;
use once_cell::sync::OnceCell;
use serde::Serialize;
use serde_json::{Map, Value as JsonValue};
use tokio::sync::mpsc;
use typst::{diag::FileResult, syntax::Source};
use typst_ts_compiler::vfs::notify::FileChangeSet;
use typst_ts_core::config::compiler::DETACHED_ENTRY;

use crate::{
    actor::{editor::EditorRequest, export::ExportConfig, typ_client::CompileClientActor},
    compile_init::{CompileConfig, ConstCompileConfig},
    harness::InitializedLspDriver,
    invalid_params, result_to_response,
    state::MemoryFileMeta,
    LspHost, LspResult,
};

use super::*;

/// The object providing the language server functionality.
pub struct CompileState {
    /// The language server client.
    pub client: LspHost<CompileState>,

    // State to synchronize with the client.
    /// Whether the server is shutting down.
    pub shutdown_requested: bool,

    // Configurations
    /// User configuration from the editor.
    pub config: CompileConfig,
    /// Const configuration initialized at the start of the session.
    /// For example, the position encoding.
    pub const_config: ConstCompileConfig,

    // Command maps
    /// Extra commands provided with `textDocument/executeCommand`.
    pub exec_cmds: ExecuteCmdMap<Self>,
    /// Regular notifications for dispatching.
    pub notify_cmds: NotifyCmdMap<Self>,
    /// Regular commands for dispatching.
    pub regular_cmds: RegularCmdMap<Self>,

    // Resources
    /// The runtime handle to spawn tasks.
    pub handle: tokio::runtime::Handle,
    /// Source synchronized with client
    pub memory_changes: HashMap<Arc<Path>, MemoryFileMeta>,
    /// The diagnostics sender to send diagnostics to `crate::actor::cluster`.
    pub editor_tx: mpsc::UnboundedSender<EditorRequest>,
    /// The compiler actor.
    pub compiler: Option<CompileClientActor>,
}

impl CompileState {
    pub fn new(
        client: LspHost<CompileState>,
        compile_config: CompileConfig,
        const_config: ConstCompileConfig,
        editor_tx: mpsc::UnboundedSender<EditorRequest>,
        handle: tokio::runtime::Handle,
    ) -> Self {
        CompileState {
            client,
            editor_tx,
            shutdown_requested: false,
            config: compile_config,
            const_config,
            compiler: None,
            handle,
            memory_changes: HashMap::new(),

            exec_cmds: Self::get_exec_commands(),
            regular_cmds: Self::get_regular_cmds(),
            notify_cmds: Self::get_notify_cmds(),
        }
    }

    pub fn const_config(&self) -> &ConstCompileConfig {
        &self.const_config
    }

    pub fn compiler(&self) -> &CompileClientActor {
        self.compiler.as_ref().unwrap()
    }

    pub fn vfs_snapshot(&self) -> FileChangeSet {
        FileChangeSet::new_inserts(
            self.memory_changes
                .iter()
                .map(|(path, meta)| {
                    let content = meta.content.clone().text().as_bytes().into();
                    (path.clone(), FileResult::Ok((meta.mt, content)).into())
                })
                .collect(),
        )
    }

    pub fn apply_vfs_snapshot(&mut self, changeset: FileChangeSet) {
        for path in changeset.removes {
            self.memory_changes.remove(&path);
        }

        for (path, file) in changeset.inserts {
            let Ok(content) = file.content() else {
                continue;
            };
            let Ok(mtime) = file.mtime() else {
                continue;
            };
            let Ok(content) = std::str::from_utf8(content) else {
                log::error!("invalid utf8 content in snapshot file: {path:?}");
                continue;
            };

            let meta = MemoryFileMeta {
                mt: *mtime,
                content: Source::new(*DETACHED_ENTRY, content.to_owned()),
            };
            self.memory_changes.insert(path, meta);
        }
    }

    #[rustfmt::skip]
    fn get_regular_cmds() -> RegularCmdMap<Self> {
        type State = CompileState;
        use lsp_types::request::*;
        RegularCmdMap::from_iter([
            request_fn_!(ExecuteCommand, State::execute_command),
        ])
    }

    fn get_notify_cmds() -> NotifyCmdMap<Self> {
        // use lsp_types::notification::*;
        NotifyCmdMap::from_iter([
            // notify_fn!(DidOpenTextDocument, Self::did_open),
            // notify_fn!(DidCloseTextDocument, Self::did_close),
            // notify_fn!(DidChangeTextDocument, Self::did_change),
            // notify_fn!(DidSaveTextDocument, Self::did_save),
            // notify_fn!(DidChangeConfiguration, Self::did_change_configuration),
        ])
    }

    pub fn schedule<T: Serialize + 'static>(
        &mut self,
        req_id: RequestId,
        resp: SchedulableResponse<T>,
    ) -> ScheduledResult {
        let resp = resp?;
        let client = self.client.clone();
        self.handle.spawn(async move {
            client.respond(result_to_response(req_id, resp.await));
        });
        Ok(Some(()))
    }
}

#[derive(Debug)]
enum Event {
    Lsp(Message),
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::Lsp(_) => write!(f, "Event::Lsp"),
        }
    }
}

impl InitializedLspDriver for CompileState {
    fn initialized(&mut self, _params: lsp_types::InitializedParams) {}

    fn main_loop(&mut self, inbox: crossbeam_channel::Receiver<Message>) -> anyhow::Result<()> {
        while let Some(event) = self.next_event(&inbox) {
            if matches!(
                &event,
                Event::Lsp(Message::Notification(Notification { method, .. }))
                if method == lsp_types::notification::Exit::METHOD
            ) {
                return Ok(());
            }
            self.handle_event(event)?;
        }

        warn!("client exited without proper shutdown sequence");
        Ok(())
    }
}

impl CompileState {
    fn next_event(&self, inbox: &Receiver<Message>) -> Option<Event> {
        select! {
            recv(inbox) -> msg =>
                msg.ok().map(Event::Lsp),
        }
    }

    fn handle_event(&mut self, event: Event) -> anyhow::Result<()> {
        let loop_start = Instant::now();

        // let was_quiescent = self.is_quiescent();
        match event {
            Event::Lsp(msg) => match msg {
                Message::Request(req) => self.on_request(loop_start, req),
                Message::Notification(not) => self.on_notification(loop_start, not)?,
                Message::Response(resp) => self.client.clone().complete_request(self, resp),
            },
        }
        Ok(())
    }

    /// Registers and handles a request. This should only be called once per
    /// incoming request.
    fn on_request(&mut self, request_received: Instant, req: Request) {
        self.client.register_request(&req, request_received);

        if self.shutdown_requested {
            self.client.respond(Response::new_err(
                req.id.clone(),
                ErrorCode::InvalidRequest as i32,
                "Shutdown already requested.".to_owned(),
            ));
            return;
        }

        let Some(handler) = self.regular_cmds.get(req.method.as_str()) else {
            warn!("unhandled request: {}", req.method);
            return;
        };

        let result = handler(self, req.id.clone(), req.params);
        match result {
            Ok(Some(())) => {}
            _ => self.client.respond(result_to_response(req.id, result)),
        }
    }

    /// Handles an incoming notification.
    fn on_notification(
        &mut self,
        request_received: Instant,
        not: Notification,
    ) -> anyhow::Result<()> {
        info!("notifying {} - at {:0.2?}", not.method, request_received);

        let Some(handler) = self.notify_cmds.get(not.method.as_str()) else {
            warn!("unhandled notification: {}", not.method);
            return Ok(());
        };

        let result = handler(self, not.params);

        let request_duration = request_received.elapsed();
        if let Err(err) = result {
            error!(
                "notifing {} failed in {:0.2?}: {:?}",
                not.method, request_duration, err
            );
        } else {
            info!(
                "notifing {} succeeded in {:0.2?}",
                not.method, request_duration
            );
        }

        Ok(())
    }
}

impl CompileState {
    pub fn on_changed_configuration(&mut self, values: Map<String, JsonValue>) -> LspResult<()> {
        let config = self.config.clone();
        match self.config.update_by_map(&values) {
            Ok(()) => {}
            Err(err) => {
                self.config = config;
                error!("error applying new settings: {err}");
                return Err(invalid_params(format!(
                    "error applying new settings: {err}"
                )));
            }
        }

        if let Some(e) = self.compiler.as_mut() {
            e.sync_config(self.config.clone());
        }

        if config.output_path != self.config.output_path
            || config.export_pdf != self.config.export_pdf
        {
            let config = ExportConfig {
                substitute_pattern: self.config.output_path.clone(),
                mode: self.config.export_pdf,
            };

            self.compiler
                .as_mut()
                .unwrap()
                .change_export_pdf(config.clone());
        }

        if config.primary_opts() != self.config.primary_opts() {
            self.config.fonts = OnceCell::new(); // todo: don't reload fonts if not changed
            self.restart_server("primary");
        }

        info!("new settings applied");
        Ok(())
    }
}
