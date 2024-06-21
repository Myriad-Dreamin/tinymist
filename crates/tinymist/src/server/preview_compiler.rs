use std::sync::Arc;

use await_tree::InstrumentAwait;

use tokio::sync::mpsc;
use typst::diag::SourceResult;
use typst::model::Document;

use typst::World;
use typst_ts_compiler::service::{CompileDriver, CompileMiddleware};
use typst_ts_compiler::service::{
    CompileExporter, Compiler, EntryReader, PureCompiler, WorldExporter,
};
use typst_ts_compiler::TypstSystemWorld;
use typst_ts_core::Error;

use typst_preview::{CompilationHandle, CompileStatus};

use crate::actor::typ_client::CompileClientActorImpl;
use crate::actor::typ_server::CompileServerActor;
use crate::compiler_init::CompileConfig;
use crate::world::{LspCompilerFeat, LspWorld};

pub type CompileService<H> =
    CompileServerActor<Reporter<CompileExporter<PureCompiler<LspWorld>>, H>, LspCompilerFeat>;
pub type CompileClient<H> =
    CompileClientActorImpl<Reporter<CompileExporter<PureCompiler<LspWorld>>, H>>;

pub struct CompileServer<H: CompilationHandle> {
    inner: CompileService<H>,
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
        world: &<C as typst_ts_compiler::service::Compiler>::W,
        env: &mut typst_ts_compiler::service::CompileEnv,
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

impl<W: World, C: Compiler<W = W> + WorldExporter<W>, H> WorldExporter<W> for Reporter<C, H> {
    fn export(&mut self, world: &W, output: Arc<typst::model::Document>) -> SourceResult<()> {
        self.inner.export(world, output)
    }
}

impl<H: CompilationHandle> CompileServer<H> {
    pub fn new(
        compiler_driver: CompileDriver<PureCompiler<TypstSystemWorld>>,
        cb: H,
        // renderer_sender: broadcast::Sender<RenderActorRequest>,
        // editor_conn_sender: mpsc::UnboundedSender<EditorActorRequest>,
    ) -> Self {
        let (intr_tx, intr_rx) = mpsc::unbounded_channel();
        let CompileDriver { compiler, universe } = compiler_driver;
        let entry = universe.entry_state();

        // CompileExporter + DynamicLayoutCompiler + WatchDriver
        let driver = CompileExporter::new(compiler);
        let driver = Reporter { inner: driver, cb };
        let inner =
            CompileServerActor::new(driver, universe, entry, intr_tx, intr_rx).with_watch(true);

        Self { inner }
    }

    pub fn spawn(self) -> Result<CompileClient<H>, Error> {
        let (export_tx, mut export_rx) = mpsc::unbounded_channel();
        let intr_tx = self.inner.intr_tx();
        let entry = self.inner.verse.entry_state();
        tokio::spawn(self.inner.spawn().instrument_await("spawn typst server"));
        // drop all export events
        tokio::spawn(async move { while let Some(_) = export_rx.recv().await {} });
        Ok(CompileClient::new(
            "main".to_owned(),
            CompileConfig::default(),
            entry,
            intr_tx,
            export_tx,
        ))
    }
}
