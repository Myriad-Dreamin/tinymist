//! The actor that runs compilations.
//!
//! ```ascii
//! ┌────────────────────────────────┐
//! │  main::compile_actor (client)  │
//! └─────┬────────────────────▲─────┘
//!       │                    │
//! ┌─────▼────────────────────┴─────┐         ┌────────────┐
//! │compiler::compile_actor (server)│◄───────►│notify_actor│
//! └─────┬────────────────────▲─────┘         └────────────┘
//!       │                    │
//! ┌─────▼────────────────────┴─────┐ handler ┌────────────┐
//! │   compiler::compile_handler    ├────────►│ rest actors│
//! └────────────────────────────────┘         └────────────┘
//! ```
//!
//! We use typst by creating a
//! [`CompileServerActor`][`crate::actor::typ_server::CompileServerActor`] and
//! running compiler with callbacking [`CompileHandler`] incrementally. An
//! additional [`CompileClientActor`] is also created to control the
//! [`CompileServerActor`][`crate::actor::typ_server::CompileServerActor`].
//!
//! The [`CompileHandler`] will push information to other actors.

use std::{
    collections::HashMap,
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{anyhow, bail};
use log::{error, info, trace};
use reflexo_typst::{
    debug_loc::DataSource, error::prelude::*, typst::prelude::*, vfs::notify::MemoryEvent,
    world::EntryState, CompileReport, EntryReader, Error, ImmutPath, TaskInputs, TypstFont,
};
use sync_lsp::{just_future, QueryFuture};
use tinymist_query::{
    analysis::{Analysis, AnalysisContext, AnalysisResources},
    CompilerQueryResponse, DiagnosticsMap, ExportKind, SemanticRequest, ServerInfoResponse,
    StatefulRequest, VersionedDocument,
};
use tinymist_render::PeriscopeRenderer;
use tokio::sync::{mpsc, oneshot};
use typst::{
    diag::{PackageError, SourceDiagnostic},
    layout::Position,
    syntax::package::PackageSpec,
    World as TypstWorld,
};

use super::{
    editor::{DocVersion, EditorRequest, TinymistCompileStatusEnum},
    typ_server::{
        CompilationHandle, CompileSnapshot, CompiledArtifact, Interrupt, SucceededArtifact,
    },
};
use crate::{
    task::{ExportTask, ExportUserConfig},
    world::{LspCompilerFeat, LspWorld},
    CompileConfig,
};

type EditorSender = mpsc::UnboundedSender<EditorRequest>;

pub struct CompileHandler {
    pub(crate) diag_group: String,
    pub(crate) analysis: Analysis,
    pub(crate) periscope: PeriscopeRenderer,

    #[cfg(feature = "preview")]
    pub(crate) inner: Arc<parking_lot::RwLock<Option<Arc<typst_preview::CompileWatcher>>>>,

    pub(crate) intr_tx: mpsc::UnboundedSender<Interrupt<LspCompilerFeat>>,
    pub(crate) export: ExportTask,
    pub(crate) editor_tx: EditorSender,

    pub(crate) notified_revision: parking_lot::Mutex<usize>,
}

impl CompileHandler {
    /// Snapshot the compiler thread for tasks
    pub fn snapshot(&self) -> ZResult<QuerySnap> {
        let (tx, rx) = oneshot::channel();
        self.intr_tx
            .send(Interrupt::SnapshotRead(tx))
            .map_err(map_string_err("failed to send snapshot request"))?;

        Ok(QuerySnap { rx })
    }

    /// Get latest artifact the compiler thread for tasks
    pub fn artifact(&self) -> ZResult<ArtifactSnap> {
        let (tx, rx) = oneshot::channel();
        self.intr_tx
            .send(Interrupt::CurrentRead(tx))
            .map_err(map_string_err("failed to send snapshot request"))?;

        Ok(ArtifactSnap { rx })
    }

    pub fn flush_compile(&self) {
        // todo: better way to flush compile
        let _ = self.intr_tx.send(Interrupt::Compile);
    }

    pub fn add_memory_changes(&self, event: MemoryEvent) {
        let _ = self.intr_tx.send(Interrupt::Memory(event));
    }

    pub fn change_task(&self, task_inputs: TaskInputs) {
        let _ = self.intr_tx.send(Interrupt::ChangeTask(task_inputs));
    }

    pub async fn settle(&self) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        let _ = self.intr_tx.send(Interrupt::Settle(tx));
        rx.await?;
        Ok(())
    }

    fn push_diagnostics(&self, revision: usize, diagnostics: Option<DiagnosticsMap>) {
        let dv = DocVersion {
            group: self.diag_group.clone(),
            revision,
        };
        let res = self.editor_tx.send(EditorRequest::Diag(dv, diagnostics));
        if let Err(err) = res {
            error!("failed to send diagnostics: {err:#}");
        }
    }

    fn notify_diagnostics(
        &self,
        world: &LspWorld,
        errors: EcoVec<SourceDiagnostic>,
        warnings: Option<EcoVec<SourceDiagnostic>>,
    ) {
        let revision = world.revision().get();
        trace!("notify diagnostics({revision}): {errors:#?} {warnings:#?}");

        let diagnostics = self.run_analysis(world, |ctx| {
            tinymist_query::convert_diagnostics(ctx, errors.iter().chain(warnings.iter().flatten()))
        });

        match diagnostics {
            Ok(diagnostics) => {
                let entry = world.entry_state();
                // todo: better way to remove diagnostics
                // todo: check all errors in this file
                let detached = entry.is_inactive();
                let valid = !detached;
                self.push_diagnostics(revision, valid.then_some(diagnostics));
            }
            Err(err) => {
                error!("TypstActor: failed to convert diagnostics: {:#}", err);
                self.push_diagnostics(revision, None);
            }
        }
    }

    pub fn run_stateful<T: StatefulRequest>(
        &self,
        snap: CompileSnapshot<LspCompilerFeat>,
        query: T,
        wrapper: fn(Option<T::Response>) -> CompilerQueryResponse,
    ) -> anyhow::Result<CompilerQueryResponse> {
        let w = &snap.world;
        let doc = snap.success_doc.map(|doc| VersionedDocument {
            version: w.revision().get(),
            document: doc,
        });
        self.run_analysis(w, |ctx| query.request(ctx, doc))
            .map(wrapper)
    }

    pub fn run_semantic<T: SemanticRequest>(
        &self,
        snap: CompileSnapshot<LspCompilerFeat>,
        query: T,
        wrapper: fn(Option<T::Response>) -> CompilerQueryResponse,
    ) -> anyhow::Result<CompilerQueryResponse> {
        self.run_analysis(&snap.world, |ctx| query.request(ctx))
            .map(wrapper)
    }

    pub fn run_analysis<T>(
        &self,
        w: &LspWorld,
        f: impl FnOnce(&mut AnalysisContext<'_>) -> T,
    ) -> anyhow::Result<T> {
        let Some(main) = w.main_id() else {
            error!("TypstActor: main file is not set");
            bail!("main file is not set");
        };
        let Some(root) = w.entry_state().root() else {
            error!("TypstActor: root is not set");
            bail!("root is not set");
        };
        w.source(main).map_err(|err| {
            info!("TypstActor: failed to prepare main file: {err:?}");
            anyhow!("failed to get source: {err}")
        })?;

        struct WrapWorld<'a>(&'a LspWorld, &'a PeriscopeRenderer);

        impl<'a> AnalysisResources for WrapWorld<'a> {
            fn world(&self) -> &dyn typst::World {
                self.0
            }

            fn resolve(&self, spec: &PackageSpec) -> Result<Arc<Path>, PackageError> {
                use reflexo_typst::world::package::PackageRegistry;
                self.0.registry.resolve(spec)
            }

            fn dependencies(&self) -> EcoVec<ImmutPath> {
                use reflexo_typst::WorldDeps;
                let mut deps = EcoVec::new();
                self.0.iter_dependencies(&mut |dep| {
                    deps.push(dep);
                });

                deps
            }

            /// Resolve extra font information.
            fn font_info(&self, font: TypstFont) -> Option<Arc<DataSource>> {
                self.0.font_resolver.describe_font(&font)
            }

            /// Get the local packages and their descriptions.
            fn local_packages(&self) -> EcoVec<PackageSpec> {
                crate::tool::package::list_package_by_namespace(
                    &self.0.registry,
                    eco_format!("local"),
                )
                .into_iter()
                .map(|(_, spec)| spec)
                .collect()
            }

            /// Resolve periscope image at the given position.
            fn periscope_at(
                &self,
                ctx: &mut AnalysisContext,
                doc: VersionedDocument,
                pos: Position,
            ) -> Option<String> {
                self.1.render_marked(ctx, doc, pos)
            }
        }

        let w = WrapWorld(w, &self.periscope);

        let mut analysis = self.analysis.snapshot(root, &w);
        Ok(f(&mut analysis))
    }

    // todo: multiple preview support
    #[cfg(feature = "preview")]
    #[must_use]
    pub fn register_preview(&self, handle: &Arc<typst_preview::CompileWatcher>) -> bool {
        let mut p = self.inner.write();
        if p.as_ref().is_some() {
            return false;
        }
        *p = Some(handle.clone());
        true
    }

    #[cfg(feature = "preview")]
    #[must_use]
    pub fn unregister_preview(&self, task_id: &str) -> bool {
        let mut p = self.inner.write();
        if p.as_ref().is_some_and(|p| p.task_id() == task_id) {
            *p = None;
            return true;
        }
        false
    }
}

impl CompilationHandle<LspCompilerFeat> for CompileHandler {
    fn status(&self, revision: usize, _rep: CompileReport) {
        // todo: seems to duplicate with CompileStatus
        let status = match _rep {
            CompileReport::Suspend => {
                self.push_diagnostics(revision, None);
                TinymistCompileStatusEnum::CompileSuccess
            }
            CompileReport::Stage(_, _, _) => TinymistCompileStatusEnum::Compiling,
            CompileReport::CompileSuccess(_, _, _) | CompileReport::CompileWarning(_, _, _) => {
                TinymistCompileStatusEnum::CompileSuccess
            }
            CompileReport::CompileError(_, _, _) | CompileReport::ExportError(_, _, _) => {
                TinymistCompileStatusEnum::CompileError
            }
        };

        let this = &self;
        this.editor_tx
            .send(EditorRequest::Status(this.diag_group.clone(), status))
            .unwrap();

        #[cfg(feature = "preview")]
        if let Some(inner) = this.inner.read().as_ref() {
            use typst_preview::CompileStatus;

            let status = match _rep {
                CompileReport::Suspend => CompileStatus::CompileSuccess,
                CompileReport::Stage(_, _, _) => CompileStatus::Compiling,
                CompileReport::CompileSuccess(_, _, _) | CompileReport::CompileWarning(_, _, _) => {
                    CompileStatus::CompileSuccess
                }
                CompileReport::CompileError(_, _, _) | CompileReport::ExportError(_, _, _) => {
                    CompileStatus::CompileError
                }
            };

            inner.status(status);
        }
    }

    fn notify_compile(&self, snap: &CompiledArtifact<LspCompilerFeat>, _rep: CompileReport) {
        // todo: we need to manage the revision for fn status() as well
        {
            let mut n_rev = self.notified_revision.lock();
            if *n_rev >= snap.world.revision().get() {
                log::info!(
                    "TypstActor: already notified for revision {} <= {n_rev}",
                    snap.world.revision(),
                );
                return;
            }
            *n_rev = snap.world.revision().get();
        }

        self.notify_diagnostics(
            &snap.world,
            snap.doc.clone().err().unwrap_or_default(),
            snap.env.tracer.as_ref().map(|e| e.clone().warnings()),
        );

        self.export.signal(snap, snap.signal);

        self.editor_tx
            .send(EditorRequest::Status(
                self.diag_group.clone(),
                if snap.doc.is_ok() {
                    TinymistCompileStatusEnum::CompileSuccess
                } else {
                    TinymistCompileStatusEnum::CompileError
                },
            ))
            .unwrap();

        #[cfg(feature = "preview")]
        if let Some(inner) = self.inner.read().as_ref() {
            let res = snap
                .doc
                .clone()
                .map_err(|_| typst_preview::CompileStatus::CompileError);
            inner.notify_compile(res, snap.signal.by_fs_events, snap.signal.by_entry_update);
        }
    }
}

pub struct CompileClientActor {
    pub handle: Arc<CompileHandler>,

    pub config: CompileConfig,
    entry: EntryState,
}

impl CompileClientActor {
    pub(crate) fn new(
        handle: Arc<CompileHandler>,
        config: CompileConfig,
        entry: EntryState,
    ) -> Self {
        Self {
            handle,
            config,
            entry,
        }
    }

    /// Snapshot the compiler thread for tasks
    pub fn snapshot(&self) -> ZResult<QuerySnap> {
        self.handle.clone().snapshot()
    }

    pub fn add_memory_changes(&self, event: MemoryEvent) {
        self.handle.add_memory_changes(event);
    }

    pub fn change_task(&self, task_inputs: TaskInputs) {
        self.handle.change_task(task_inputs);
    }

    pub fn sync_config(&mut self, config: CompileConfig) {
        self.config = config;
    }

    pub(crate) fn change_export_config(&mut self, config: ExportUserConfig) {
        self.handle.export.change_config(config);
    }

    pub fn on_export(&self, kind: ExportKind, path: PathBuf) -> QueryFuture {
        let snap = self.snapshot()?;

        let entry = self.config.determine_entry(Some(path.as_path().into()));
        let export = self.handle.export.oneshot(snap, Some(entry), kind);
        just_future(async move {
            let res = export.await?;

            log::info!("CompileActor: on export end: {path:?} as {res:?}");
            Ok(tinymist_query::CompilerQueryResponse::OnExport(res))
        })
    }
}

impl CompileClientActor {
    pub async fn settle(&mut self) {
        let _ = self.change_entry(None);
        info!("TypstActor({}): settle requested", self.handle.diag_group);
        match self.handle.settle().await {
            Ok(()) => info!("TypstActor({}): settled", self.handle.diag_group),
            Err(err) => error!(
                "TypstActor({}): failed to settle: {err:#}",
                self.handle.diag_group
            ),
        }
    }

    pub fn change_entry(&mut self, path: Option<ImmutPath>) -> Result<bool, Error> {
        if path
            .as_deref()
            .is_some_and(|p| !p.is_absolute() && !p.starts_with("/untitled"))
        {
            return Err(error_once!("entry file must be absolute", path: path.unwrap().display()));
        }

        let next_entry = self.config.determine_entry(path);
        if next_entry == self.entry {
            return Ok(false);
        }

        let diag_group = &self.handle.diag_group;
        info!("the entry file of TypstActor({diag_group}) is changing to {next_entry:?}");

        self.change_task(TaskInputs {
            entry: Some(next_entry.clone()),
            ..Default::default()
        });

        self.entry = next_entry;

        Ok(true)
    }

    pub fn clear_cache(&self) {
        self.handle.analysis.clear_cache();
    }

    pub fn collect_server_info(&self) -> QueryFuture {
        let dg = self.handle.diag_group.clone();

        let snap = self.snapshot()?;
        just_future(async move {
            let snap = snap.receive().await?;
            let w = &snap.world;

            let info = ServerInfoResponse {
                root: w.entry_state().root().map(|e| e.as_ref().to_owned()),
                font_paths: w.font_resolver.font_paths().to_owned(),
                inputs: w.inputs().as_ref().deref().clone(),
                estimated_memory_usage: HashMap::from_iter([
                    // todo: vfs memory usage
                    // ("vfs".to_owned(), w.vfs.read().memory_usage()),
                    // todo: analysis memory usage
                    // ("analysis".to_owned(), cc.analysis.estimated_memory()),
                ]),
            };

            let info = Some(HashMap::from_iter([(dg, info)]));
            Ok(tinymist_query::CompilerQueryResponse::ServerInfo(info))
        })
    }
}

pub struct QuerySnap {
    rx: oneshot::Receiver<CompileSnapshot<LspCompilerFeat>>,
}

impl QuerySnap {
    /// Snapshot the compiler thread for tasks
    pub async fn receive(self) -> ZResult<CompileSnapshot<LspCompilerFeat>> {
        self.rx
            .await
            .map_err(map_string_err("failed to get snapshot"))
    }
}

pub struct ArtifactSnap {
    rx: oneshot::Receiver<SucceededArtifact<LspCompilerFeat>>,
}

impl ArtifactSnap {
    /// Get latest artifact the compiler thread for tasks
    pub async fn receive(self) -> ZResult<SucceededArtifact<LspCompilerFeat>> {
        self.rx
            .await
            .map_err(map_string_err("failed to get snapshot"))
    }
}
