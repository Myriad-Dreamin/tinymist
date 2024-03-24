//! Bootstrap actors for Tinymist.

pub mod cluster;
pub mod compile;
pub mod render;
pub mod typst;

use ::typst::diag::FileResult;
use tokio::sync::{broadcast, watch};
use typst_ts_compiler::vfs::notify::FileChangeSet;
use typst_ts_core::config::compiler::EntryState;

use self::{
    render::{PdfExportActor, PdfExportConfig},
    typst::{create_server, CompileActor},
};
use crate::TypstLanguageServer;

impl TypstLanguageServer {
    pub fn server(&self, name: String, entry: EntryState) -> CompileActor {
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
        create_server(
            name,
            &self.config,
            self.const_config(),
            self.font.clone(),
            entry,
            snapshot,
            self.diag_tx.clone(),
            doc_tx,
            render_tx,
        )
    }
}
