use std::{borrow::Cow, path::PathBuf, sync::Arc};

use parking_lot::Mutex;
use tinymist_query::DiagnosticsMap;
use tokio::sync::{broadcast, mpsc, watch};
use typst_ts_core::{config::CompileOpts, TypstDocument};

use self::{
    render::{PdfExportActor, RenderActorRequest},
    typst::{create_server, CompileCluster, CompileDriver},
};
use crate::{ConstConfig, LspHost};

pub mod render;
pub mod typst;

struct Repr {
    config: ConstConfig,

    diag_tx: mpsc::UnboundedSender<(String, DiagnosticsMap)>,
    diag_rx: Option<mpsc::UnboundedReceiver<(String, DiagnosticsMap)>>,
    doc_tx: Option<watch::Sender<Option<Arc<TypstDocument>>>>,
    doc_rx: watch::Receiver<Option<Arc<TypstDocument>>>,
    render_tx: broadcast::Sender<RenderActorRequest>,
}

pub struct ActorFactory(Arc<Mutex<Repr>>);

impl ActorFactory {
    pub fn new(config: ConstConfig) -> Self {
        let (diag_tx, diag_rx) = mpsc::unbounded_channel();
        let (doc_sender, doc_rx) = watch::channel(None);
        let (render_tx, _) = broadcast::channel(10);

        Self(Arc::new(Mutex::new(Repr {
            config,
            diag_tx,
            diag_rx: Some(diag_rx),
            doc_tx: Some(doc_sender),
            doc_rx,
            render_tx,
        })))
    }

    pub async fn pdf_export_actor(&self) {
        let this = self.0.lock();
        tokio::spawn(PdfExportActor::new(this.doc_rx.clone(), this.render_tx.subscribe()).run());
    }

    pub fn prepare_cluster(&self, host: LspHost, roots: Vec<PathBuf>) -> CompileCluster {
        let mut this = self.0.lock();

        let doc_tx = this.doc_tx.take().expect("doc_sender is poisoned");
        let diag_rx = this.diag_rx.take().expect("diag_rx is poisoned");

        let opts = CompileOpts {
            root_dir: roots.first().cloned().unwrap_or_default(),
            // todo: font paths
            // font_paths: arguments.font_paths.clone(),
            with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
            ..CompileOpts::default()
        };
        let primary = create_server(
            "primary".to_owned(),
            &this.config,
            CompileDriver::new(roots.clone(), opts),
            this.diag_tx.clone(),
            doc_tx,
            this.render_tx.clone(),
        );

        CompileCluster::new(host, &this.config, primary, diag_rx)
    }
}
