//! Bootstrap actors for Tinymist.

pub mod editor;
#[cfg(feature = "preview")]
pub mod preview;
pub mod typ_client;
pub mod typ_server;

use std::sync::Arc;

use reflexo::ImmutPath;
use reflexo_typst::vfs::notify::{FileChangeSet, MemoryEvent};
use reflexo_typst::world::EntryState;
use tinymist_query::analysis::{Analysis, PeriscopeProvider};
use tinymist_query::{ExportKind, LocalContext, VersionedDocument};
use tinymist_render::PeriscopeRenderer;
use tokio::sync::mpsc;
use typst::layout::Position;

use crate::{
    task::{ExportConfig, ExportTask, ExportUserConfig},
    world::{ImmutDict, LspUniverseBuilder},
    LanguageState,
};
use typ_client::{CompileClientActor, CompileHandler};
use typ_server::{CompileServerActor, CompileServerOpts};

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
        let const_config = self.const_config();
        let periscope_args = self.compile_config().periscope_args.clone();
        let handle = Arc::new(CompileHandler {
            #[cfg(feature = "preview")]
            inner: std::sync::Arc::new(parking_lot::RwLock::new(None)),
            diag_group: editor_group.clone(),
            intr_tx: intr_tx.clone(),
            export: export.clone(),
            editor_tx: self.editor_tx.clone(),
            stats: Default::default(),
            analysis: Arc::new(Analysis {
                position_encoding: const_config.position_encoding,
                allow_overlapping_token: const_config.tokens_overlapping_token_support,
                allow_multiline_token: const_config.tokens_multiline_token_support,
                remove_html: !self.config.support_html_in_markdown,
                completion_feat: self.config.completion.clone(),
                color_theme: match self.compile_config().color_theme.as_deref() {
                    Some("dark") => tinymist_query::ColorTheme::Dark,
                    _ => tinymist_query::ColorTheme::Light,
                },
                periscope: periscope_args.map(|args| {
                    let r = TypstPeriscopeProvider(PeriscopeRenderer::new(args));
                    Arc::new(r) as Arc<dyn PeriscopeProvider + Send + Sync>
                }),
                tokens_caches: Arc::default(),
                workers: Default::default(),
                caches: Default::default(),
                analysis_rev_cache: Arc::default(),
                stats: Arc::default(),
            }),

            notified_revision: parking_lot::Mutex::new(0),
        });

        let font_resolver = self.compile_config().determine_fonts();
        let entry_ = entry.clone();
        let compile_handle = handle.clone();
        let cache = self.cache.clone();
        let cert_path = self.compile_config().determine_certification_path();
        let package = self.compile_config().determine_package_opts();

        self.client.handle.spawn_blocking(move || {
            // Create the world
            let font_resolver = font_resolver.wait().clone();
            let package_registry =
                LspUniverseBuilder::resolve_package(cert_path.clone(), Some(&package));
            let verse =
                LspUniverseBuilder::build(entry_.clone(), inputs, font_resolver, package_registry)
                    .expect("incorrect options");

            // Create the actor
            let server = CompileServerActor::new_with(
                verse,
                intr_tx,
                intr_rx,
                CompileServerOpts {
                    compile_handle,
                    cache,
                    ..Default::default()
                },
            )
            .with_watch(true);
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

struct TypstPeriscopeProvider(PeriscopeRenderer);

impl PeriscopeProvider for TypstPeriscopeProvider {
    /// Resolve periscope image at the given position.
    fn periscope_at(
        &self,
        ctx: &mut LocalContext,
        doc: VersionedDocument,
        pos: Position,
    ) -> Option<String> {
        self.0.render_marked(ctx, doc, pos)
    }
}
