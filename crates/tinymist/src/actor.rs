//! Bootstrap actors for Tinymist.

pub mod cluster;
pub mod render;
pub mod typ_client;
pub mod typ_server;

use tokio::sync::{broadcast, watch};
use typst::{diag::FileResult, util::Deferred};
use typst_ts_compiler::{
    service::CompileDriverImpl,
    vfs::notify::{FileChangeSet, MemoryEvent},
};
use typst_ts_core::config::compiler::EntryState;

use self::{
    render::{PdfExportActor, PdfExportConfig},
    typ_client::{CompileClientActor, CompileDriver, CompileHandler},
    typ_server::CompileServerActor,
};
use crate::{
    world::{LspWorld, LspWorldBuilder},
    TypstLanguageServer,
};

type CompileDriverInner = CompileDriverImpl<LspWorld>;

impl TypstLanguageServer {
    pub fn server(&self, diag_group: String, entry: EntryState) -> CompileClientActor {
        let (doc_tx, doc_rx) = watch::channel(None);
        let (render_tx, _) = broadcast::channel(10);

        // Run the PDF export actor before preparing cluster to avoid loss of events
        tokio::spawn(
            PdfExportActor::new(
                doc_rx.clone(),
                render_tx.subscribe(),
                PdfExportConfig {
                    substitute_pattern: self.config.output_path.clone(),
                    entry: entry.clone(),
                    mode: self.config.export_pdf,
                },
            )
            .run(),
        );

        // Take all dirty files in memory as the initial snapshot
        let snapshot = FileChangeSet::new_inserts(
            self.memory_changes
                .iter()
                .map(|(path, meta)| {
                    let content = meta.content.clone().text().as_bytes().into();
                    (path.clone(), FileResult::Ok((meta.mt, content)).into())
                })
                .collect(),
        );

        // Create the server
        let inner = Deferred::new({
            let current_runtime = tokio::runtime::Handle::current();
            let handler = CompileHandler {
                #[cfg(feature = "preview")]
                inner: std::sync::Arc::new(parking_lot::Mutex::new(None)),
                diag_group: diag_group.clone(),
                doc_tx,
                render_tx: render_tx.clone(),
                diag_tx: self.diag_tx.clone(),
            };

            let position_encoding = self.const_config().position_encoding;
            let diag_group = diag_group.clone();
            let entry = entry.clone();
            let font_resolver = self.font.clone();
            move || {
                log::info!("TypstActor: creating server for {diag_group}");

                // Create the world
                let font_resolver = font_resolver.wait().clone();
                let world = LspWorldBuilder::build(entry.clone(), font_resolver)
                    .expect("incorrect options");

                // Create the compiler
                let driver = CompileDriverInner::new(world);
                let driver = CompileDriver {
                    inner: driver,
                    handler,
                    position_encoding,
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

        CompileClientActor::new(diag_group, self.config.clone(), entry, inner, render_tx)
    }
}
