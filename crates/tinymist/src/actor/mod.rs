use std::{borrow::Cow, path::PathBuf, sync::Arc};

use ::typst::util::Deferred;
use parking_lot::Mutex;
use tinymist_query::DiagnosticsMap;
use tokio::sync::{broadcast, mpsc, watch};
use typst_ts_core::config::CompileOpts;

use self::{
    render::PdfExportActor,
    typst::{create_server, CompileCluster, CompileHandler, CompileNode},
};
use crate::{ConstConfig, LspHost};

pub mod render;
pub mod typst;

struct Repr {
    config: ConstConfig,

    diag_tx: mpsc::UnboundedSender<(String, Option<DiagnosticsMap>)>,
    diag_rx: Option<mpsc::UnboundedReceiver<(String, Option<DiagnosticsMap>)>>,
}

impl Repr {
    fn server(
        &mut self,
        name: String,
        roots: Vec<PathBuf>,
        entry: Option<PathBuf>,
    ) -> Deferred<CompileNode<CompileHandler>> {
        let (doc_tx, doc_rx) = watch::channel(None);
        let (render_tx, _) = broadcast::channel(10);

        // Run the PDF export actor before preparing cluster to avoid loss of events
        tokio::spawn(PdfExportActor::new(doc_rx.clone(), render_tx.subscribe()).run());

        let opts = CompileOpts {
            root_dir: roots.first().cloned().unwrap_or_default(),
            // todo: font paths
            // font_paths: arguments.font_paths.clone(),
            with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
            ..CompileOpts::default()
        };
        create_server(
            name,
            &self.config,
            roots.clone(),
            opts,
            entry,
            self.diag_tx.clone(),
            doc_tx,
            render_tx,
        )
    }

    pub fn prepare_cluster(
        &mut self,
        fac: ActorFactory,
        host: LspHost,
        roots: Vec<PathBuf>,
    ) -> CompileCluster {
        let diag_rx = self.diag_rx.take().expect("diag_rx is poisoned");

        let primary = self.server("primary".to_owned(), roots.clone(), None);
        CompileCluster::new(fac, host, roots, &self.config, primary, diag_rx)
    }
}

#[derive(Clone)]
pub struct ActorFactory(Arc<Mutex<Repr>>);

impl ActorFactory {
    pub fn new(config: ConstConfig) -> Self {
        let (diag_tx, diag_rx) = mpsc::unbounded_channel();

        Self(Arc::new(Mutex::new(Repr {
            config,
            diag_tx,
            diag_rx: Some(diag_rx),
        })))
    }

    fn server(
        &self,
        name: String,
        roots: Vec<PathBuf>,
        entry: Option<PathBuf>,
    ) -> Deferred<CompileNode<CompileHandler>> {
        self.0.lock().server(name, roots, entry)
    }

    pub fn prepare_cluster(&self, host: LspHost, roots: Vec<PathBuf>) -> CompileCluster {
        self.0.lock().prepare_cluster(self.clone(), host, roots)
    }
}
