use std::sync::Arc;

use await_tree::InstrumentAwait;

use parking_lot::lock_api::RwLock;
use tinymist_query::analysis::Analysis;
use tinymist_query::PositionEncoding;
use tokio::sync::{mpsc, watch};
use typst_preview::CompilationHandleImpl;
use typst_ts_core::Error;

use crate::actor::typ_client::CompileHandler;
use crate::actor::typ_server::CompileServerActor;
use crate::world::LspCompilerFeat;
use crate::LspUniverse;

pub type CompileService = CompileServerActor<LspCompilerFeat>;

pub struct CompileServer {
    inner: CompileService,
    handle: Arc<CompileHandler>,
}

impl CompileServer {
    pub fn new(verse: LspUniverse, cb: CompilationHandleImpl) -> Self {
        // type EditorSender = mpsc::UnboundedSender<EditorRequest>;
        let (doc_tx, _) = watch::channel(None);
        let (export_tx, mut export_rx) = mpsc::unbounded_channel();
        let (editor_tx, mut editor_rx) = mpsc::unbounded_channel();
        let (intr_tx, intr_rx) = mpsc::unbounded_channel();

        let handle = Arc::new(CompileHandler {
            inner: std::sync::Arc::new(RwLock::new(Some(cb))),
            diag_group: "main".to_owned(),
            intr_tx: intr_tx.clone(),
            doc_tx,
            export_tx,
            editor_tx,
            analysis: Analysis {
                position_encoding: PositionEncoding::Utf16,
                enable_periscope: false,
                caches: Default::default(),
            },
            periscope: tinymist_render::PeriscopeRenderer::default(),
        });

        // Consume export_tx and editor_rx
        tokio::spawn(async move { while export_rx.recv().await.is_some() {} });
        tokio::spawn(async move { while editor_rx.recv().await.is_some() {} });

        let inner =
            CompileServerActor::new(verse, intr_tx, intr_rx).with_watch(Some(handle.clone()));

        Self { inner, handle }
    }

    pub fn spawn(self) -> Result<Arc<CompileHandler>, Error> {
        tokio::spawn(self.inner.spawn().instrument_await("spawn typst server"));
        Ok(self.handle.clone())
    }
}
