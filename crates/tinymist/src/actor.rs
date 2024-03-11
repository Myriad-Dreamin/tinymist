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

        // todo: don't ignore entry from typst_extra_args
        // entry: command.input,
        let roots = self.roots.clone();
        let root_dir = self.config.root_path.clone();
        let root_dir = root_dir.or_else(|| {
            self.config
                .typst_extra_args
                .as_ref()
                .and_then(|x| x.root_dir.clone())
        });
        let root_dir = root_dir.unwrap_or_else(|| roots.first().cloned().unwrap());

        // Run the PDF export actor before preparing cluster to avoid loss of events
        tokio::spawn(
            PdfExportActor::new(
                doc_rx.clone(),
                render_tx.subscribe(),
                PdfExportConfig {
                    substitute_pattern: self.config.output_path.clone(),
                    root: root_dir.clone().into(),
                    path: entry.clone().map(From::from),
                    mode: self.config.export_pdf,
                },
            )
            .run(),
        );

        let mut opts = CompileOpts {
            root_dir,
            // todo: additional inputs
            with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
            ..self.compile_opts.clone()
        };

        if let Some(extras) = &self.config.typst_extra_args {
            if let Some(inputs) = extras.inputs.as_ref() {
                if opts.inputs.is_empty() {
                    opts.inputs = inputs.clone();
                }
            }
            if !extras.font_paths.is_empty() && opts.font_paths.is_empty() {
                opts.font_paths = extras.font_paths.clone();
            }
        }

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
