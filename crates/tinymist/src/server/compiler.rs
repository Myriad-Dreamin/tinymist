use core::fmt;
use std::{collections::HashMap, path::Path, sync::Arc, time::Instant};

use crossbeam_channel::{select, Receiver};
use log::{error, info, warn};
use lsp_server::{Notification, Request, ResponseError};
use lsp_types::{notification::Notification as _, ExecuteCommandParams};
use paste::paste;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};
use tinymist_query::{ExportKind, PageSelection};
use tokio::sync::mpsc;
use typst::{diag::FileResult, syntax::Source, util::Deferred};
use typst_ts_compiler::vfs::notify::FileChangeSet;
use typst_ts_core::{config::compiler::DETACHED_ENTRY, ImmutPath};

use crate::{
    actor::{cluster::CompileClusterRequest, render::ExportConfig, typ_client::CompileClientActor},
    compiler_init::{CompileConfig, CompilerConstConfig},
    harness::InitializedLspDriver,
    internal_error, invalid_params, method_not_found, run_query,
    state::MemoryFileMeta,
    world::SharedFontResolver,
    LspHost, LspResult,
};

type LspMethod<Res> = fn(srv: &mut CompileServer, args: JsonValue) -> LspResult<Res>;
type LspHandler<Req, Res> = fn(srv: &mut CompileServer, args: Req) -> LspResult<Res>;

type ExecuteCmdMap = HashMap<&'static str, LspHandler<Vec<JsonValue>, JsonValue>>;
type NotifyCmdMap = HashMap<&'static str, LspMethod<()>>;
type RegularCmdMap = HashMap<&'static str, LspMethod<JsonValue>>;

#[macro_export]
macro_rules! exec_fn {
    ($ty: ty, Self::$method: ident, $($arg_key:ident),+ $(,)?) => {{
        const E: $ty = |this, $($arg_key),+| this.$method($($arg_key),+);
        E
    }};
}

#[macro_export]
macro_rules! request_fn {
    ($desc: ty, Self::$method: ident) => {
        (<$desc>::METHOD, {
            const E: LspMethod<JsonValue> = |this, req| {
                let req: <$desc as lsp_types::request::Request>::Params =
                    serde_json::from_value(req).unwrap(); // todo: soft unwrap
                let res = this.$method(req)?;
                let res = serde_json::to_value(res).unwrap(); // todo: soft unwrap
                Ok(res)
            };
            E
        })
    };
}

#[macro_export]
macro_rules! notify_fn {
    ($desc: ty, Self::$method: ident) => {
        (<$desc>::METHOD, {
            const E: LspMethod<()> = |this, input| {
                let input: <$desc as lsp_types::notification::Notification>::Params =
                    serde_json::from_value(input).unwrap(); // todo: soft unwrap
                this.$method(input)
            };
            E
        })
    };
}

pub struct CompileServerArgs {
    pub client: LspHost<CompileServer>,
    pub compile_config: CompileConfig,
    pub const_config: CompilerConstConfig,
    pub diag_tx: mpsc::UnboundedSender<CompileClusterRequest>,
    pub font: Deferred<SharedFontResolver>,
    pub handle: tokio::runtime::Handle,
}

/// The object providing the language server functionality.
pub struct CompileServer {
    /// The language server client.
    pub client: LspHost<CompileServer>,

    // State to synchronize with the client.
    /// Whether the server is shutting down.
    pub shutdown_requested: bool,

    // Configurations
    /// User configuration from the editor.
    pub config: CompileConfig,
    /// Const configuration initialized at the start of the session.
    /// For example, the position encoding.
    pub const_config: CompilerConstConfig,

    // Command maps
    /// Extra commands provided with `textDocument/executeCommand`.
    pub exec_cmds: ExecuteCmdMap,
    /// Regular notifications for dispatching.
    pub notify_cmds: NotifyCmdMap,
    /// Regular commands for dispatching.
    pub regular_cmds: RegularCmdMap,

    // Resources
    /// The runtime handle to spawn tasks.
    pub handle: tokio::runtime::Handle,
    /// The font resolver to use.
    pub font: Deferred<SharedFontResolver>,
    /// Source synchronized with client
    pub memory_changes: HashMap<Arc<Path>, MemoryFileMeta>,
    /// The diagnostics sender to send diagnostics to `crate::actor::cluster`.
    pub diag_tx: mpsc::UnboundedSender<CompileClusterRequest>,
    /// The compiler actor.
    pub compiler: Option<CompileClientActor>,
}

impl CompileServer {
    pub fn new(args: CompileServerArgs) -> Self {
        let CompileServerArgs {
            client,
            compile_config,
            const_config,
            diag_tx,
            font,
            handle,
        } = args;

        CompileServer {
            client,
            diag_tx,
            shutdown_requested: false,
            config: compile_config,
            const_config,
            font,
            compiler: None,
            handle,
            memory_changes: HashMap::new(),

            exec_cmds: Self::get_exec_commands(),
            regular_cmds: Self::get_regular_cmds(),
            notify_cmds: Self::get_notify_cmds(),
        }
    }

    pub fn const_config(&self) -> &CompilerConstConfig {
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
    fn get_regular_cmds() -> RegularCmdMap {
        use lsp_types::request::*;
        RegularCmdMap::from_iter([
            request_fn!(ExecuteCommand, Self::execute_command),
        ])
    }

    fn get_notify_cmds() -> NotifyCmdMap {
        // use lsp_types::notification::*;
        NotifyCmdMap::from_iter([
            // notify_fn!(DidOpenTextDocument, Self::did_open),
            // notify_fn!(DidCloseTextDocument, Self::did_close),
            // notify_fn!(DidChangeTextDocument, Self::did_change),
            // notify_fn!(DidSaveTextDocument, Self::did_save),
            // notify_fn!(DidChangeConfiguration, Self::did_change_configuration),
        ])
    }
}

#[derive(Debug)]
enum Event {
    Lsp(lsp_server::Message),
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Event::Lsp(_) => write!(f, "Event::Lsp"),
        }
    }
}

impl InitializedLspDriver for CompileServer {
    fn initialized(&mut self, _params: lsp_types::InitializedParams) {}

    fn main_loop(
        &mut self,
        inbox: crossbeam_channel::Receiver<lsp_server::Message>,
    ) -> anyhow::Result<()> {
        while let Some(event) = self.next_event(&inbox) {
            if matches!(
                &event,
                Event::Lsp(lsp_server::Message::Notification(Notification { method, .. }))
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

impl CompileServer {
    fn next_event(&self, inbox: &Receiver<lsp_server::Message>) -> Option<Event> {
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
                lsp_server::Message::Request(req) => self.on_new_request(loop_start, req),
                lsp_server::Message::Notification(not) => self.on_notification(loop_start, not)?,
                lsp_server::Message::Response(resp) => {
                    self.client.clone().complete_request(self, resp)
                }
            },
        }
        Ok(())
    }

    /// Registers and handles a request. This should only be called once per
    /// incoming request.
    fn on_new_request(&mut self, request_received: Instant, req: Request) {
        self.client.register_request(&req, request_received);
        self.on_request(req);
    }

    /// Handles a request.
    fn on_request(&mut self, req: Request) {
        if self.shutdown_requested {
            self.client.respond(lsp_server::Response::new_err(
                req.id.clone(),
                lsp_server::ErrorCode::InvalidRequest as i32,
                "Shutdown already requested.".to_owned(),
            ));
            return;
        }

        let Some(handler) = self.regular_cmds.get(req.method.as_str()) else {
            warn!("unhandled request: {}", req.method);
            return;
        };

        let result = handler(self, req.params);

        if let Ok(response) = result_to_response(req.id, result) {
            self.client.respond(response);
        }

        fn result_to_response(
            id: lsp_server::RequestId,
            result: Result<JsonValue, ResponseError>,
        ) -> Result<lsp_server::Response, Cancelled> {
            let res = match result {
                Ok(resp) => lsp_server::Response::new_ok(id, resp),
                Err(e) => lsp_server::Response::new_err(id, e.code, e.message),
            };
            Ok(res)
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

impl CompileServer {
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

        // todo: watch changes of the root path

        if config.output_path != self.config.output_path
            || config.export_pdf != self.config.export_pdf
        {
            let config = ExportConfig {
                substitute_pattern: self.config.output_path.clone(),
                mode: self.config.export_pdf,
                ..ExportConfig::default()
            };

            self.compiler
                .as_mut()
                .unwrap()
                .change_export_pdf(config.clone());
        }

        info!("new settings applied");
        Ok(())
    }
}

struct Cancelled;

impl CompileServer {
    fn get_exec_commands() -> ExecuteCmdMap {
        macro_rules! redirected_command {
            ($key: expr, Self::$method: ident) => {
                (
                    $key,
                    exec_fn!(LspHandler<Vec<JsonValue>, JsonValue>, Self::$method, inputs),
                )
            };
        }

        ExecuteCmdMap::from_iter([
            redirected_command!("tinymist.exportPdf", Self::export_pdf),
            redirected_command!("tinymist.exportSvg", Self::export_svg),
            redirected_command!("tinymist.exportPng", Self::export_png),
            redirected_command!("tinymist.doClearCache", Self::clear_cache),
            redirected_command!("tinymist.changeEntry", Self::change_entry),
        ])
    }

    /// The entry point for the `workspace/executeCommand` request.
    fn execute_command(&mut self, params: ExecuteCommandParams) -> LspResult<Option<JsonValue>> {
        let ExecuteCommandParams {
            command,
            arguments,
            work_done_progress_params: _,
        } = params;
        let Some(handler) = self.exec_cmds.get(command.as_str()) else {
            error!("asked to execute unknown command");
            return Err(method_not_found());
        };

        Ok(Some(handler(self, arguments)?))
    }

    /// Export the current document as a PDF file.
    pub fn export_pdf(&self, arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        self.export(ExportKind::Pdf, arguments)
    }

    /// Export the current document as a Svg file.
    pub fn export_svg(&self, arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        let opts = parse_opts(arguments.get(1))?;
        self.export(ExportKind::Svg { page: opts.page }, arguments)
    }

    /// Export the current document as a Png file.
    pub fn export_png(&self, arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        let opts = parse_opts(arguments.get(1))?;
        self.export(ExportKind::Png { page: opts.page }, arguments)
    }

    /// Export the current document as some format. The client is responsible
    /// for passing the correct absolute path of typst document.
    pub fn export(&self, kind: ExportKind, arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        let path = parse_path(arguments.first())?.as_ref().to_owned();

        let res = run_query!(self.OnExport(path, kind))?;
        let res = serde_json::to_value(res).map_err(|_| internal_error("Cannot serialize path"))?;

        Ok(res)
    }

    /// Clear all cached resources.
    ///
    /// # Errors
    /// Errors if the cache could not be cleared.
    pub fn clear_cache(&self, _arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        comemo::evict(0);
        Ok(JsonValue::Null)
    }

    /// Focus main file to some path.
    pub fn change_entry(&mut self, arguments: Vec<JsonValue>) -> LspResult<JsonValue> {
        let new_entry = parse_path_or_null(arguments.first())?;

        let update_result = self.do_change_entry(new_entry.clone());
        update_result.map_err(|err| internal_error(format!("could not focus file: {err}")))?;

        info!("entry changed: {entry:?}", entry = new_entry);
        Ok(JsonValue::Null)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExportOpts {
    page: PageSelection,
}

fn parse_opts(v: Option<&JsonValue>) -> LspResult<ExportOpts> {
    Ok(match v {
        Some(opts) => serde_json::from_value::<ExportOpts>(opts.clone())
            .map_err(|_| invalid_params("The third argument is not a valid object"))?,
        _ => ExportOpts {
            page: PageSelection::First,
        },
    })
}

fn parse_path(v: Option<&JsonValue>) -> LspResult<ImmutPath> {
    let new_entry = match v {
        Some(JsonValue::String(s)) => Path::new(s).into(),
        _ => {
            return Err(invalid_params(
                "The first parameter is not a valid path or null",
            ))
        }
    };

    Ok(new_entry)
}

fn parse_path_or_null(v: Option<&JsonValue>) -> LspResult<Option<ImmutPath>> {
    match v {
        Some(JsonValue::Null) => Ok(None),
        v => Ok(Some(parse_path(v)?)),
    }
}
