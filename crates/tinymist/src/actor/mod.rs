//! Bootstrap actors for Tinymist.

pub mod editor;
pub mod export;
pub mod format;
pub mod typ_client;
pub mod typ_server;
pub mod user_action;

use std::sync::Arc;

use tinymist_query::analysis::Analysis;
use tinymist_query::ExportKind;
use tinymist_render::PeriscopeRenderer;
use tokio::sync::{mpsc, watch};
use typst_ts_compiler::vfs::notify::{FileChangeSet, MemoryEvent};
use typst_ts_core::config::compiler::EntryState;

use self::{
    export::{ExportActor, ExportConfig},
    format::run_format_thread,
    typ_client::{CompileClientActor, CompileHandler},
    typ_server::CompileServerActor,
    user_action::run_user_action_thread,
};
use crate::{
    compile::CompileState,
    world::{ImmutDict, LspWorldBuilder},
    LanguageState,
};

impl CompileState {
    pub fn restart_server(&mut self, group: &str) {
        let server = self.server(
            group.to_owned(),
            self.config
                .determine_entry(self.config.determine_default_entry_path()),
            self.config.determine_inputs(),
            self.vfs_snapshot(),
        );
        if let Some(mut previous_server) = self.compiler.replace(server) {
            std::thread::spawn(move || previous_server.settle());
        }
    }

    pub fn server(
        &self,
        editor_group: String,
        entry: EntryState,
        inputs: ImmutDict,
        snapshot: FileChangeSet,
    ) -> CompileClientActor {
        let (doc_tx, doc_rx) = watch::channel(None);
        let (export_tx, export_rx) = mpsc::unbounded_channel();
        let (intr_tx, intr_rx) = mpsc::unbounded_channel();
        let intr_tx_ = intr_tx.clone();

        // Run Export actors before preparing cluster to avoid loss of events
        self.client.handle.spawn(
            ExportActor {
                group: editor_group.clone(),
                editor_tx: self.editor_tx.clone(),
                export_rx,
                doc_rx,
                entry: entry.clone(),
                config: ExportConfig {
                    substitute_pattern: self.config.output_path.clone(),
                    mode: self.config.export_pdf,
                },
                kind: ExportKind::Pdf,
                count_words: self.config.notify_compile_status,
            }
            .run(),
        );

        log::info!(
            "TypstActor: creating server for {editor_group}, entry: {entry:?}, inputs: {inputs:?}"
        );

        // Create the compile handler for client consuming results.
        let position_encoding = self.const_config().position_encoding;
        let enable_periscope = self.config.periscope_args.is_some();
        let periscope_args = self.config.periscope_args.clone();
        let handle = Arc::new(CompileHandler {
            #[cfg(feature = "preview")]
            inner: std::sync::Arc::new(None),
            diag_group: editor_group.clone(),
            doc_tx,
            export_tx: export_tx.clone(),
            editor_tx: self.editor_tx.clone(),
            analysis: Analysis {
                position_encoding,
                enable_periscope,
                caches: Default::default(),
            },
            periscope: PeriscopeRenderer::new(periscope_args.unwrap_or_default()),
        });

        let font_resolver = self.config.determine_fonts();
        let entry_ = entry.clone();
        let handle_ = handle.clone();

        self.client.handle.spawn_blocking(move || {
            // Create the world
            let font_resolver = font_resolver.wait().clone();
            let verse = LspWorldBuilder::build(entry_.clone(), font_resolver, inputs)
                .expect("incorrect options");

            // Create the actor
            let server = CompileServerActor::new(verse, intr_tx, intr_rx).with_watch(Some(handle_));
            tokio::spawn(server.spawn());
        });

        // Create the client
        let config = self.config.clone();
        let client = CompileClientActor::new(handle, config, entry, intr_tx_);
        // We do send memory changes instead of initializing compiler with them.
        // This is because there are state recorded inside of the compiler actor, and we
        // must update them.
        client.add_memory_changes(MemoryEvent::Update(snapshot));
        client
    }
}

impl LanguageState {
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
        self.format_thread = Some(tx_req);

        let client = self.client.clone();
        let mode = self.config.formatter;
        let enc = self.const_config.position_encoding;
        let config = format::FormatConfig { mode, width: 120 };
        std::thread::spawn(move || run_format_thread(config, rx_req, client, enc));
    }

    pub fn run_user_action_thread(&mut self) {
        if self.user_action_thread.is_some() {
            log::error!("user action threads are already started");
            return;
        }

        let (tx_req, rx_req) = crossbeam_channel::unbounded();
        self.user_action_thread = Some(tx_req);

        let client = self.client.clone();
        std::thread::spawn(move || run_user_action_thread(rx_req, client));
    }
}
