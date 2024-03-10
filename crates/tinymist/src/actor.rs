//! Bootstrap actors for Tinymist.

pub mod cluster;
pub mod compile;
pub mod render;
pub mod typst;

use std::{borrow::Cow, path::PathBuf};

use ::typst::util::Deferred;
use tokio::sync::{broadcast, watch};
use typst_ts_core::config::CompileOpts;

use self::{
    render::{PdfExportActor, PdfExportConfig},
    typst::{create_server, CompileActor},
};
use crate::TypstLanguageServer;

impl TypstLanguageServer {
    pub fn server(&self, name: String, entry: Option<PathBuf>) -> Deferred<CompileActor> {
        let (doc_tx, doc_rx) = watch::channel(None);
        let (render_tx, _) = broadcast::channel(10);

        // Run the PDF export actor before preparing cluster to avoid loss of events
        tokio::spawn(
            PdfExportActor::new(
                doc_rx.clone(),
                render_tx.subscribe(),
                Some(PdfExportConfig {
                    path: entry
                        .as_ref()
                        .map(|e| e.clone().with_extension("pdf").into()),
                    mode: self.config.export_pdf,
                }),
            )
            .run(),
        );

        let roots = self.roots.clone();
        let opts = CompileOpts {
            root_dir: roots.first().cloned().unwrap_or_default(),
            // todo: font paths
            // font_paths: arguments.font_paths.clone(),
            with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
            ..CompileOpts::default()
        };
        create_server(
            name,
            self.const_config(),
            roots,
            opts,
            entry,
            self.diag_tx.clone(),
            doc_tx,
            render_tx,
        )
    }
}
