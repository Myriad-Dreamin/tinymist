use std::sync::Arc;

use await_tree::InstrumentAwait;

use tokio::sync::mpsc;
use typst::diag::SourceResult;
use typst::model::Document;

use typst::World;
use typst_ts_compiler::{CompileDriver, CompileMiddleware};
use typst_ts_compiler::{CompileExporter, Compiler, PureCompiler};
use typst_ts_compiler::{EntryReader, TypstSystemWorld};
use typst_ts_core::Error;

use typst_preview::{CompilationHandle, CompileStatus};

use crate::actor::typ_client::CompileClientActorImpl;
use crate::actor::typ_server::CompileServerActor;
use crate::compile_init::CompileConfig;
use crate::world::LspCompilerFeat;

pub type CompileService = CompileServerActor<LspCompilerFeat>;
pub type CompileClient = CompileClientActorImpl;

pub struct CompileServer {
    inner: CompileService,
}

pub struct Reporter<C, H> {
    inner: C,
    cb: H,
}

impl<C: Compiler, H: CompilationHandle> CompileMiddleware for Reporter<C, H> {
    type Compiler = C;

    fn inner(&self) -> &Self::Compiler {
        &self.inner
    }

    fn inner_mut(&mut self) -> &mut Self::Compiler {
        &mut self.inner
    }

    fn wrap_compile(
        &mut self,
        world: &<C as typst_ts_compiler::Compiler>::W,
        env: &mut typst_ts_compiler::CompileEnv,
    ) -> SourceResult<Arc<Document>> {
        self.cb.status(CompileStatus::Compiling);
        match self.inner_mut().compile(world, env) {
            Ok(doc) => {
                self.cb.notify_compile(Ok(doc.clone()));
                Ok(doc)
            }
            Err(err) => {
                self.cb.notify_compile(Err(CompileStatus::CompileError));
                Err(err)
            }
        }
    }
}

impl CompileServer {
    pub fn new<H: CompilationHandle>(
        compiler_driver: CompileDriver<PureCompiler<TypstSystemWorld>>,
        cb: H,
        // renderer_sender: broadcast::Sender<RenderActorRequest>,
        // editor_conn_sender: mpsc::UnboundedSender<EditorActorRequest>,
    ) -> Self {
        // let (intr_tx, intr_rx) = mpsc::unbounded_channel();
        // let CompileDriver { compiler, universe } = compiler_driver;
        // let entry = universe.entry_state();

        // // CompileExporter + DynamicLayoutCompiler + WatchDriver
        // let driver = CompileExporter::new(compiler);
        // let driver = Reporter { inner: driver, cb };
        // let inner =
        //     CompileServerActor::new(driver, universe, entry, intr_tx,
        // intr_rx).with_watch(true);

        // Self { inner }

        todo!()
    }

    pub fn spawn(self) -> Result<CompileClient, Error> {
        let (export_tx, mut export_rx) = mpsc::unbounded_channel();
        let intr_tx = self.inner.intr_tx.clone();
        let entry = self.inner.verse.entry_state();
        tokio::spawn(self.inner.spawn().instrument_await("spawn typst server"));
        // drop all export events
        tokio::spawn(async move { while export_rx.recv().await.is_some() {} });
        Ok(CompileClient::new(
            "main".to_owned(),
            CompileConfig::default(),
            entry,
            intr_tx,
            export_tx,
        ))
    }
}
