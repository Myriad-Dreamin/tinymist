//! Bootstrap actors for Tinymist.

pub mod cluster;
pub mod compile;
pub mod render;
pub mod typst;

use std::path::PathBuf;

use ::typst::diag::FileResult;
use tokio::sync::{broadcast, watch};
use typst_ts_compiler::vfs::notify::FileChangeSet;
use typst_ts_core::ImmutPath;

use self::{
    render::{PdfExportActor, PdfExportConfig},
    typst::{create_server, CompileActor, OptsState},
};
use crate::{world::CompileOnceOpts, TypstLanguageServer};

impl TypstLanguageServer {
    pub fn server(&self, name: String, entry: Option<ImmutPath>) -> CompileActor {
        let (doc_tx, doc_rx) = watch::channel(None);
        let (render_tx, _) = broadcast::channel(10);

        // todo: don't ignore entry from typst_extra_args
        // entry: command.input,
        let root_dir = self.config.determine_root(entry.as_ref());

        // Run the PDF export actor before preparing cluster to avoid loss of events
        tokio::spawn(
            PdfExportActor::new(
                doc_rx.clone(),
                render_tx.subscribe(),
                PdfExportConfig {
                    substitute_pattern: self.config.output_path.clone(),
                    root: root_dir.clone(),
                    path: entry.clone().map(From::from),
                    mode: self.config.export_pdf,
                },
            )
            .run(),
        );

        let opts = {
            let mut opts = self.compile_opts.clone();

            if let Some(extras) = &self.config.typst_extra_args {
                if let Some(inputs) = extras.inputs.as_ref() {
                    if opts.inputs.is_empty() {
                        opts.inputs = inputs.clone();
                    }
                }
            }

            move |root_dir: PathBuf| CompileOnceOpts {
                // todo: additional inputs
                root_dir,
                ..opts
            }
        };

        let snapshot = FileChangeSet::new_inserts(
            self.memory_changes
                .iter()
                .map(|(path, meta)| {
                    let content = meta.content.clone().text().as_bytes().into();
                    (path.clone(), FileResult::Ok((meta.mt, content)).into())
                })
                .collect(),
        );

        create_server(
            name,
            &self.config,
            self.const_config(),
            OptsState::new(root_dir.clone(), opts),
            self.font.clone(),
            root_dir,
            entry,
            snapshot,
            self.diag_tx.clone(),
            doc_tx,
            render_tx,
        )
    }
}
