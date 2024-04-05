//! Bootstrap actors for Tinymist.

pub mod cluster;
mod formatting;
pub mod render;
pub mod typ_client;
pub mod typ_server;
mod user_action;

use std::path::Path;

use tinymist_query::analysis::Analysis;
use tinymist_query::ExportKind;
use tinymist_render::PeriscopeRenderer;
use tokio::sync::{broadcast, watch};
use typst::util::Deferred;
use typst_ts_compiler::{
    service::CompileDriverImpl,
    vfs::notify::{FileChangeSet, MemoryEvent},
};
use typst_ts_core::config::compiler::EntryState;

use self::{
    formatting::run_format_thread,
    render::{ExportActor, ExportConfig},
    typ_client::{CompileClientActor, CompileDriver, CompileHandler},
    typ_server::CompileServerActor,
    user_action::run_user_action_thread,
};
use crate::{
    compiler::CompileServer,
    world::{ImmutDict, LspWorld, LspWorldBuilder},
    ExportMode, TypstLanguageServer,
};

pub use formatting::{FormattingConfig, FormattingRequest};
pub use user_action::{UserActionRequest, UserActionTraceRequest};

type CompileDriverInner = CompileDriverImpl<LspWorld>;

impl CompileServer {
    pub fn server(
        &self,
        editor_group: String,
        entry: EntryState,
        inputs: ImmutDict,
        snapshot: FileChangeSet,
    ) -> CompileClientActor {
        let (doc_tx, doc_rx) = watch::channel(None);
        let (render_tx, _) = broadcast::channel(10);

        let config = ExportConfig {
            substitute_pattern: self.config.output_path.clone(),
            entry: entry.clone(),
            mode: self.config.export_pdf,
        };

        // Run Export actors before preparing cluster to avoid loss of events
        self.handle.spawn(
            ExportActor::new(
                editor_group.clone(),
                doc_rx.clone(),
                self.diag_tx.clone(),
                render_tx.subscribe(),
                config.clone(),
                ExportKind::Pdf,
            )
            .run(),
        );
        if self.config.notify_compile_status {
            let mut config = config;
            config.mode = ExportMode::OnType;
            self.handle.spawn(
                ExportActor::new(
                    editor_group.clone(),
                    doc_rx.clone(),
                    self.diag_tx.clone(),
                    render_tx.subscribe(),
                    config,
                    ExportKind::WordCount,
                )
                .run(),
            );
        }

        // Create the server
        let inner = Deferred::new({
            let current_runtime = self.handle.clone();
            let handler = CompileHandler {
                #[cfg(feature = "preview")]
                inner: std::sync::Arc::new(parking_lot::Mutex::new(None)),
                diag_group: editor_group.clone(),
                doc_tx,
                render_tx: render_tx.clone(),
                editor_tx: self.diag_tx.clone(),
            };

            let position_encoding = self.const_config().position_encoding;
            let enable_periscope = self.config.periscope_args.is_some();
            let periscope_args = self.config.periscope_args.clone();
            let diag_group = editor_group.clone();
            let entry = entry.clone();
            let font_resolver = self.font.clone();
            move || {
                log::info!("TypstActor: creating server for {diag_group}, entry: {entry:?}, inputs: {inputs:?}");

                // Create the world
                let font_resolver = font_resolver.wait().clone();
                let world = LspWorldBuilder::build(entry.clone(), font_resolver, inputs)
                    .expect("incorrect options");

                // Create the compiler
                let driver = CompileDriverInner::new(world);
                let driver = CompileDriver {
                    inner: driver,
                    handler,
                    analysis: Analysis {
                        position_encoding,
                        root: Path::new("").into(),
                        enable_periscope,
                        caches: Default::default(),
                    },
                    periscope: PeriscopeRenderer::new(periscope_args.unwrap_or_default()),
                };

                // Create the actor
                let actor = CompileServerActor::new(driver, entry).with_watch(true);
                let (server, client) = actor.split();

                // We do send memory changes instead of initializing compiler with them.
                // This is because there are state recorded inside of the compiler actor, and we
                // must update them.
                client.add_memory_changes(MemoryEvent::Update(snapshot));

                current_runtime.spawn(server.spawn());

                client
            }
        });

        CompileClientActor::new(editor_group, self.config.clone(), entry, inner, render_tx)
    }
}

impl TypstLanguageServer {
    pub fn server(
        &self,
        diag_group: String,
        entry: EntryState,
        inputs: ImmutDict,
    ) -> CompileClientActor {
        // Take all dirty files in memory as the initial snapshot
        self.primary
            .server(diag_group, entry, inputs, self.primary.vfs_snapshot())
    }

    pub fn run_format_thread(&mut self) {
        if self.format_thread.is_some() {
            log::error!("formatting thread is already started");
            return;
        }

        let (tx_req, rx_req) = crossbeam_channel::unbounded();
        self.format_thread = Some(tx_req.clone());

        let client = self.client.clone();
        let mode = self.config.formatter;
        let enc = self.const_config.position_encoding;
        std::thread::spawn(move || {
            run_format_thread(FormattingConfig { mode, width: 120 }, rx_req, client, enc)
        });
    }

    pub fn run_user_action_thread(&mut self) {
        if self.user_action_threads.is_some() {
            log::error!("user action threads are already started");
            return;
        }

        let (tx_req, rx_req) = crossbeam_channel::unbounded();
        self.user_action_threads = Some(tx_req.clone());

        let client = self.client.clone();
        std::thread::spawn(move || run_user_action_thread(rx_req, client));
    }
}
