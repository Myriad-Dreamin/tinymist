use std::sync::Arc;

use await_tree::InstrumentAwait;

use tinymist_query::analysis::Analysis;
use tinymist_query::PositionEncoding;
use tokio::sync::{mpsc, watch};
use typst_preview::CompilationHandleImpl;
use typst_ts_compiler::EntryReader;
use typst_ts_core::Error;

use crate::actor::typ_client::{CompileClientActor, CompileHandler};
use crate::actor::typ_server::CompileServerActor;
use crate::compile_init::CompileConfig;
use crate::harness::AnyLspHost;
use crate::world::LspCompilerFeat;
use crate::LspUniverse;

pub type CompileService = CompileServerActor<LspCompilerFeat>;
pub type CompileClient = CompileClientActor;

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

        let handle = Arc::new(CompileHandler {
            inner: std::sync::Arc::new(Some(cb)),
            diag_group: "main".to_owned(),
            doc_tx,
            export_tx,
            editor_tx,
            analysis: Analysis {
                position_encoding: PositionEncoding::Utf16,
                enable_periscope: false,
                caches: Default::default(),
            },
            periscope: tinymist_render::PeriscopeRenderer::default(),
            lsp_tx: AnyLspHost::default(),
        });

        // Consume export_tx and editor_rx
        tokio::spawn(async move { while export_rx.recv().await.is_some() {} });
        tokio::spawn(async move { while editor_rx.recv().await.is_some() {} });

        let (intr_tx, intr_rx) = mpsc::unbounded_channel();

        let inner =
            CompileServerActor::new(verse, intr_tx, intr_rx).with_watch(Some(handle.clone()));

        Self { inner, handle }
    }

    pub fn spawn(self) -> Result<CompileClient, Error> {
        let intr_tx = self.inner.intr_tx.clone();
        let entry = self.inner.verse.entry_state();
        tokio::spawn(self.inner.spawn().instrument_await("spawn typst server"));
        Ok(CompileClient::new(
            self.handle,
            CompileConfig::default(),
            entry,
            intr_tx,
        ))
    }
}
