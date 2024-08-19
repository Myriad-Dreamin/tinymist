//! Bootstrap actors for Tinymist.

pub mod editor;
#[cfg(feature = "preview")]
pub mod preview;
pub mod typ_client;
pub mod typ_server;

use std::sync::Arc;

use reflexo::ImmutPath;
use tinymist_query::analysis::Analysis;
use tinymist_query::ExportKind;
use tinymist_render::PeriscopeRenderer;
use tokio::sync::mpsc;
use typ_server::CompileServerOpts;
use typst_ts_compiler::vfs::notify::{FileChangeSet, MemoryEvent};
use typst_ts_core::config::compiler::EntryState;

use self::{
    typ_client::{CompileClientActor, CompileHandler},
    typ_server::CompileServerActor,
};
use crate::{
    task::{ExportConfig, ExportTask, ExportUserConfig},
    world::{ImmutDict, LspUniverseBuilder},
    LanguageState,
};

impl LanguageState {
    /// Restart the primary server.
    pub fn restart_primary(&mut self) {
        let entry = self.compile_config().determine_default_entry_path();
        self.restart_server("primary", entry);
    }

    /// Restart the server with the given group.
    pub fn restart_dedicate(&mut self, dedicate: &str, entry: Option<ImmutPath>) {
        self.restart_server(dedicate, entry);
    }

    /// Restart the server with the given group.
    fn restart_server(&mut self, group: &str, entry: Option<ImmutPath>) {
        let server = self.server(
            group.to_owned(),
            self.compile_config().determine_entry(entry),
            self.compile_config().determine_inputs(),
            self.vfs_snapshot(),
        );

        let prev = if group == "primary" {
            self.primary.replace(server)
        } else {
            let cell = self
                .dedicates
                .iter_mut()
                .find(|dedicate| dedicate.handle.diag_group == group);
            if let Some(dedicate) = cell {
                Some(std::mem::replace(dedicate, server))
            } else {
                self.dedicates.push(server);
                None
            }
        };

        if let Some(mut prev) = prev {
            self.client.handle.spawn(async move { prev.settle().await });
        }
    }

    /// Create a new server for the given group.
    pub fn server(
        &self,
        editor_group: String,
        entry: EntryState,
        inputs: ImmutDict,
        snapshot: FileChangeSet,
    ) -> CompileClientActor {
        let (intr_tx, intr_rx) = mpsc::unbounded_channel();

        // Run Export actors before preparing cluster to avoid loss of events
        let export = ExportTask::new(ExportConfig {
            group: editor_group.clone(),
            editor_tx: Some(self.editor_tx.clone()),
            config: ExportUserConfig {
                output: self.compile_config().output_path.clone(),
                mode: self.compile_config().export_pdf,
            },
            kind: ExportKind::Pdf {
                creation_timestamp: self.config.compile.determine_creation_timestamp(),
            },
            count_words: self.config.compile.notify_status,
        });

        log::info!(
            "TypstActor: creating server for {editor_group}, entry: {entry:?}, inputs: {inputs:?}"
        );

        // Create the compile handler for client consuming results.
        let position_encoding = self.const_config().position_encoding;
        let enable_periscope = self.compile_config().periscope_args.is_some();
        let periscope_args = self.compile_config().periscope_args.clone();
        let handle = Arc::new(CompileHandler {
            #[cfg(feature = "preview")]
            inner: std::sync::Arc::new(parking_lot::RwLock::new(None)),
            diag_group: editor_group.clone(),
            intr_tx: intr_tx.clone(),
            export: export.clone(),
            editor_tx: self.editor_tx.clone(),
            analysis: Analysis {
                position_encoding,
                enable_periscope,
                caches: Default::default(),
                workers: Default::default(),
            },
            periscope: PeriscopeRenderer::new(periscope_args.unwrap_or_default()),

            notified_revision: parking_lot::Mutex::new(0),
        });

        let font_resolver = self.compile_config().determine_fonts();
        let entry_ = entry.clone();
        let handle_ = handle.clone();
        let cache = self.cache.clone();

        self.client.handle.spawn_blocking(move || {
            // Create the world
            let font_resolver = font_resolver.wait().clone();
            let verse = LspUniverseBuilder::build(entry_.clone(), font_resolver, inputs)
                .expect("incorrect options");

            // Create the actor
            let server = CompileServerActor::new_with(
                verse,
                intr_tx,
                intr_rx,
                CompileServerOpts {
                    cache,
                    ..Default::default()
                },
            )
            .with_watch(Some(handle_));
            tokio::spawn(server.run());
        });

        // Create the client
        let config = self.compile_config().clone();
        let client = CompileClientActor::new(handle, config, entry);
        // We do send memory changes instead of initializing compiler with them.
        // This is because there are state recorded inside of the compiler actor, and we
        // must update them.
        client.add_memory_changes(MemoryEvent::Update(snapshot));
        client
    }
}
