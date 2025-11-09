//! The actor that handles various document export, like PDF and SVG export.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, OnceLock};
use std::{ops::DerefMut, pin::Pin};

use reflexo::ImmutPath;
use reflexo_typst::{Bytes, CompilationTask, ExportComputation};
use sync_ls::{internal_error, just_future};
use tinymist_project::LspWorld;
use tinymist_query::{OnExportRequest, OnExportResponse, PagedExportResponse, GLOBAL_STATS};
use tinymist_std::error::prelude::*;
use tinymist_std::fs::paths::write_atomic;
use tinymist_std::path::PathClean;
use tinymist_std::typst::TypstDocument;
use tinymist_task::{
    output_template, DocumentQuery, ExportMarkdownTask, ExportPngTask, ExportSvgTask, ExportTarget,
    ImageOutput, PdfExport, PngExport, SvgExport, TextExport,
};
use tokio::sync::mpsc;
use typlite::{Format, Typlite};
use typst::ecow::EcoString;

use futures::Future;
use parking_lot::Mutex;
use rayon::Scope;

use super::SyncTaskFactory;
use crate::lsp::query::QueryFuture;
use crate::project::{
    update_lock, ApplyProjectTask, CompiledArtifact, DevEvent, DevExportEvent, EntryReader,
    ExportHtmlTask, ExportPdfTask, ExportTask as ProjectExportTask, ExportTeXTask, ExportTextTask,
    LspCompiledArtifact, LspComputeGraph, ProjectClient, ProjectTask, TaskWhen,
    PROJECT_ROUTE_USER_ACTION_PRIORITY,
};
use crate::world::TaskInputs;
use crate::ServerState;
use crate::{actor::editor::EditorRequest, tool::word_count};

impl ServerState {
    /// Exports the current document.
    pub fn on_export(&mut self, req: OnExportRequest) -> QueryFuture {
        let OnExportRequest {
            path,
            task,
            open,
            write,
        } = req;
        let entry = self.entry_resolver().resolve(Some(path.as_path().into()));
        let lock_dir = self.entry_resolver().resolve_lock(&entry);

        let update_dep = lock_dir.clone().map(|lock_dir| {
            |snap: LspComputeGraph| async move {
                let mut updater = update_lock(lock_dir.clone());
                let world = snap.world();
                // todo: rootless.
                let root_dir = world.entry_state().root()?;
                let doc_id = updater.compiled(world, (&root_dir, &lock_dir))?;

                updater.update_materials(doc_id.clone(), world.depended_fs_paths());
                updater.route(doc_id, PROJECT_ROUTE_USER_ACTION_PRIORITY);

                updater.commit();

                Some(())
            }
        });

        let snap = self.snapshot().map_err(internal_error)?;
        just_future(async move {
            let snap = snap.task(TaskInputs {
                entry: Some(entry),
                ..TaskInputs::default()
            });

            let id = snap.world().main_id();
            let _guard = GLOBAL_STATS.stat(id, "export");

            let is_html = matches!(task, ProjectTask::ExportHtml { .. });
            // todo: we may get some file missing errors here
            let artifact = CompiledArtifact::from_graph(snap.clone(), is_html);

            let res = if write {
                // Export to file and return path
                ExportTask::do_export(task, artifact, lock_dir)
                    .await
                    .map_err(internal_error)?
            } else {
                // Export to memory and return base64-encoded data
                ExportTask::do_export_to_memory(task, artifact)
                    .await
                    .map_err(internal_error)?
            };

            if let Some(update_dep) = update_dep {
                tokio::spawn(update_dep(snap));
            }

            // Only open the first page if multiple pages are exported
            if open {
                match &res {
                    Some(OnExportResponse::Single {
                        path: Some(path), ..
                    }) => {
                        open_external(path);
                    }
                    Some(OnExportResponse::Paged { items, .. }) => {
                        if let Some(first_page) = items.first() {
                            if let Some(path) = &first_page.path {
                                open_external(path);
                            }
                        }
                    }
                    None => {
                        log::warn!("CompileActor: on export end: no export result to open");
                    }
                    _ => {}
                }
            }

            log::trace!("CompileActor: on export end: {path:?} as {res:?}");
            Ok(tinymist_query::CompilerQueryResponse::OnExport(res))
        })
    }
}

/// Runs a export document task.
#[derive(Clone)]
pub struct ExportTask {
    /// The handle running the task.
    pub handle: tokio::runtime::Handle,
    /// The editor request sender.
    pub editor_tx: Option<mpsc::UnboundedSender<EditorRequest>>,
    /// The task factory for export.
    pub factory: SyncTaskFactory<ExportUserConfig>,
    export_folder: FutureFolder,
    count_word_folder: FutureFolder,
}

impl ExportTask {
    /// Creates a new export task.
    pub fn new(
        handle: tokio::runtime::Handle,
        editor_tx: Option<mpsc::UnboundedSender<EditorRequest>>,
        export_config: ExportUserConfig,
    ) -> Self {
        Self {
            handle,
            editor_tx,
            factory: SyncTaskFactory::new(export_config),
            export_folder: FutureFolder::default(),
            count_word_folder: FutureFolder::default(),
        }
    }

    /// Changes the export configuration.
    pub fn change_config(&self, config: ExportUserConfig) {
        self.factory.mutate(|data| *data = config);
    }

    pub(crate) fn signal(
        &self,
        snap: &LspCompiledArtifact,
        client: &std::sync::Arc<dyn ProjectClient + 'static>,
    ) {
        let config = self.factory.task();

        self.signal_export(snap, &config, client);
        self.signal_count_word(snap, &config);
    }

    fn signal_export(
        &self,
        artifact: &LspCompiledArtifact,
        config: &Arc<ExportUserConfig>,
        client: &std::sync::Arc<dyn ProjectClient + 'static>,
    ) -> Option<()> {
        let doc = artifact.doc.as_ref()?;
        let s = artifact.snap.signal;

        let when = config.task.when().unwrap_or(&TaskWhen::Never);
        let need_export = match when {
            TaskWhen::Never => false,
            TaskWhen::Script => s.by_entry_update,
            TaskWhen::OnType => s.by_mem_events,
            TaskWhen::OnSave => s.by_fs_events,
            TaskWhen::OnDocumentHasTitle => s.by_fs_events && doc.info().title.is_some(),
        };

        let export_hook = config.development.then_some({
            let client = client.clone();

            let event = DevEvent::Export(DevExportEvent {
                id: artifact.id().to_string(),
                when: when.clone(),
                need_export,
                signal: s,
                path: config
                    .task
                    .as_export()
                    .and_then(|t| t.output.clone())
                    .map(|p| p.to_string()),
            });

            move || client.dev_event(event)
        });

        if !need_export {
            if let Some(f) = export_hook {
                f()
            }
            return None;
        }
        log::info!(
            "ExportTask(when={when:?}): export for {} with signal: {s:?}",
            artifact.id()
        );
        let rev = artifact.world().revision().get();
        let fut = self.export_folder.spawn(rev, || {
            let task = config.task.clone();
            let artifact = artifact.clone();
            Box::pin(async move {
                log_err(Self::do_export(task, artifact, None).await);
                if let Some(f) = export_hook {
                    f()
                }
                Some(())
            })
        })?;

        self.handle.spawn(fut);

        Some(())
    }

    fn signal_count_word(
        &self,
        artifact: &LspCompiledArtifact,
        config: &Arc<ExportUserConfig>,
    ) -> Option<()> {
        if !config.count_words {
            return None;
        }

        let editor_tx = self.editor_tx.clone()?;
        let rev = artifact.world().revision().get();
        let fut = self.count_word_folder.spawn(rev, || {
            let artifact = artifact.clone();
            Box::pin(async move {
                let id = artifact.id().clone();
                let doc = artifact.doc?;
                let wc =
                    log_err(FutureFolder::compute(move |_| word_count::word_count(&doc)).await);
                log::debug!("WordCount({id:?}:{rev}): {wc:?}");

                if let Some(wc) = wc {
                    let _ = editor_tx.send(EditorRequest::WordCount(id, wc));
                }

                Some(())
            })
        })?;

        self.handle.spawn(fut);

        Some(())
    }

    fn prepare_output_path(task: &ProjectTask, graph: &LspComputeGraph) -> Result<Option<PathBuf>> {
        let entry = graph.snap.world.entry_state();
        let config = task.as_export().unwrap();
        let output = config.output.clone().unwrap_or_default();
        let Some(write_to) = output.substitute(&entry) else {
            return Ok(None);
        };
        let write_to = if write_to.is_relative() {
            let cwd = std::env::current_dir().context("failed to get current directory")?;
            cwd.join(write_to).clean()
        } else {
            write_to.to_path_buf()
        };
        if write_to.is_relative() {
            bail!("ExportTask({task:?}): output path is relative: {write_to:?}");
        }
        if write_to.is_dir() {
            bail!("ExportTask({task:?}): output path is a directory: {write_to:?}");
        }

        // Apply page template if any
        let write_to = match task {
            ProjectTask::ExportPng(ExportPngTask {
                page_number_template: Some(page_number_template),
                ..
            })
            | ProjectTask::ExportSvg(ExportSvgTask {
                page_number_template: Some(page_number_template),
                ..
            }) => write_to.with_file_name(page_number_template),
            _ => write_to,
        };
        let write_to = write_to.with_extension(task.extension());

        Ok(Some(write_to))
    }

    /// Exports a document to memory, returning the binary data directly.
    pub async fn do_export_to_memory(
        task: ProjectTask,
        artifact: LspCompiledArtifact,
    ) -> Result<Option<OnExportResponse>> {
        use base64::prelude::*;

        let CompiledArtifact { graph, .. } = &artifact;

        let write_to = Self::prepare_output_path(&task, graph)?;

        let artifact = Self::do_export_bytes(task, artifact, 0).await?;

        let res = match artifact {
            ExportArtifact::Single(data) => OnExportResponse::Single {
                path: write_to.clone(),
                data: Some(BASE64_STANDARD.encode(data.as_slice())),
            },
            ExportArtifact::Paged { total_pages, items } => {
                let can_handle_multiple = write_to.as_ref().is_some_and(|write_to| {
                    output_template::has_indexable_template(write_to.to_str().unwrap_or_default())
                });

                OnExportResponse::Paged {
                    total_pages,
                    items: items
                        .into_iter()
                        .map(|(page_idx, bytes)| {
                            let to = write_to.as_ref().map(|write_to| {
                                if can_handle_multiple {
                                    let storage = output_template::format(
                                        write_to.to_str().unwrap_or_default(),
                                        page_idx + 1,
                                        total_pages,
                                    );
                                    PathBuf::from(storage)
                                } else {
                                    write_to.clone()
                                }
                            });

                            PagedExportResponse {
                                page: page_idx,
                                path: to,
                                data: Some(BASE64_STANDARD.encode(bytes.as_slice())),
                            }
                        })
                        .collect(),
                }
            }
        };

        Ok(Some(res))
    }

    /// Exports a document.
    pub async fn do_export(
        task: ProjectTask,
        artifact: LspCompiledArtifact,
        lock_dir: Option<ImmutPath>,
    ) -> Result<Option<OnExportResponse>> {
        let CompiledArtifact { graph, .. } = &artifact;

        let Some(write_to) = Self::prepare_output_path(&task, graph)? else {
            return Ok(None);
        };

        static EXPORT_ID: AtomicUsize = AtomicUsize::new(0);
        let export_id = EXPORT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        log::debug!(
            "ExportTask({export_id},lock={lock_dir:?}): exporting {entry:?} to {write_to:?}",
            entry = graph.snap.world.entry_state()
        );
        if let Some(e) = write_to.parent() {
            if !e.exists() {
                std::fs::create_dir_all(e).context("failed to create directory")?;
            }
        }

        let _: Option<()> = lock_dir.and_then(|lock_dir| {
            let mut updater = crate::project::update_lock(lock_dir.clone());
            let root = graph.world().entry_state().root()?;

            let doc_id = updater.compiled(graph.world(), (&root, &lock_dir))?;

            updater.task(ApplyProjectTask {
                id: doc_id.clone(),
                document: doc_id.clone(),
                task: task.clone(),
            });
            updater.update_materials(doc_id.clone(), graph.world().depended_fs_paths());
            updater.route(doc_id, PROJECT_ROUTE_USER_ACTION_PRIORITY);
            updater.commit();

            Some(())
        });

        // Generate the data using common logic
        let artifact = Self::do_export_bytes(task.clone(), artifact, export_id).await?;

        let res = match artifact {
            ExportArtifact::Single(data) => {
                let res = OnExportResponse::Single {
                    path: Some(write_to.clone()),
                    data: None,
                };

                let to = write_to.clone();
                tokio::task::spawn_blocking(move || write_atomic(to, data))
                    .await
                    .context_ut("failed to export")??;

                res
            }
            ExportArtifact::Paged { total_pages, items } => {
                let can_handle_multiple =
                    output_template::has_indexable_template(write_to.to_str().unwrap_or_default());

                if !can_handle_multiple && items.len() > 1 {
                    bail!("cannot export multiple images without a page number template ({{p}}, {{0p}}) in the output path");
                }

                let mut res_items = Vec::new();
                let mut write_futures = Vec::new();
                for (page_idx, bytes) in items {
                    let to = if can_handle_multiple {
                        let storage = output_template::format(
                            write_to.to_str().unwrap_or_default(),
                            page_idx + 1,
                            total_pages,
                        );
                        PathBuf::from(storage)
                    } else {
                        write_to.clone()
                    };

                    res_items.push(PagedExportResponse {
                        page: page_idx,
                        path: Some(to.clone()),
                        data: None,
                    });

                    let fut = tokio::task::spawn_blocking(move || write_atomic(to, bytes));
                    write_futures.push(fut);
                }

                // Await all writes in parallel
                for result in futures::future::join_all(write_futures).await {
                    result.context_ut("failed to export")??;
                }

                OnExportResponse::Paged {
                    total_pages,
                    items: res_items,
                }
            }
        };

        log::debug!("ExportTask({export_id}): export complete");
        Ok(Some(res))
    }

    /// Export a document into bytes.
    async fn do_export_bytes(
        task: ProjectTask,
        artifact: LspCompiledArtifact,
        export_id: usize,
    ) -> Result<ExportArtifact> {
        use reflexo_vec2svg::DefaultExportFeature;
        use ProjectTask::*;

        let CompiledArtifact { graph, doc, .. } = artifact;

        // Prepare the document.
        let doc = doc.context("cannot export with compilation errors")?;

        // Prepare data.
        let data = FutureFolder::compute(move |_| -> Result<ExportArtifact> {
            let doc = &doc;

            // static BLANK: Lazy<Page> = Lazy::new(Page::default);
            // todo: check warnings and errors inside
            let html_once = OnceLock::new();
            let html_doc = || -> Result<_> {
                html_once
                    .get_or_init(|| -> Result<_> {
                        Ok(match &doc {
                            TypstDocument::Html(html_doc) => html_doc.clone(),
                            TypstDocument::Paged(_) => extra_compile_for_export(graph.world())?,
                        })
                    })
                    .as_ref()
                    .map_err(|e| e.clone())
            };
            let page_once = OnceLock::new();
            let paged_doc = || {
                page_once
                    .get_or_init(|| -> Result<_> {
                        Ok(match &doc {
                            TypstDocument::Paged(paged_doc) => paged_doc.clone(),
                            TypstDocument::Html(_) => extra_compile_for_export(graph.world())?,
                        })
                    })
                    .as_ref()
                    .map_err(|e| e.clone())
            };
            let total_pages = || paged_doc().map(|d| d.pages.len()).unwrap_or_default();

            Ok(match task {
                Preview(..) => Bytes::new([]).into(),
                // todo: more pdf flags
                ExportPdf(config) => PdfExport::run(&graph, paged_doc()?, &config)?.into(),
                ExportSvg(config) => SvgExport::run(&graph, paged_doc()?, &config)?.with_pages(total_pages()),
                ExportPng(config) => PngExport::run(&graph, paged_doc()?,& config)?.with_pages(total_pages()),
                Query(config) => DocumentQuery::run(&graph, paged_doc()?, &config)??.into(),
                ExportHtml(ExportHtmlTask { export: _ }) =>
                    typst_html::html(html_doc()?)
                        .map_err(|e| format!("export error: {e:?}"))
                        .context_ut("failed to export to html")?.into(),
                ExportSvgHtml(ExportHtmlTask { export: _ }) =>
                    reflexo_vec2svg::render_svg_html::<DefaultExportFeature>(paged_doc()?).into(),
                ExportText(ExportTextTask { export: _ }) => TextExport::run_on_doc(doc)?.into(),
                ExportMd(ExportMarkdownTask {
                    processor,
                    assets_path,
                    export: _,
                }) => {
                    let conv = Typlite::new(Arc::new(graph.world().clone()))
                        .with_format(Format::Md)
                        .with_feature(typlite::TypliteFeat {
                            processor,
                            assets_path,
                            ..Default::default()
                        })
                        .convert()
                        .map_err(|e| anyhow::anyhow!("failed to convert to markdown: {e}"))?;
                    conv.into()
                }
                // todo: duplicated code with ExportMd
                ExportTeX(ExportTeXTask {
                    processor,
                    assets_path,
                    export: _,
                }) => {
                    log::info!("ExportTask({export_id}): exporting to TeX with processor {processor:?} and assets path {assets_path:?}");
                    let conv = Typlite::new(Arc::new(graph.world().clone()))
                        .with_format(Format::LaTeX)
                        .with_feature(typlite::TypliteFeat {
                            processor,
                            assets_path,
                            ..Default::default()
                        })
                        .convert()
                        .map_err(|e| anyhow::anyhow!("failed to convert to latex: {e}"))?;
                    conv.into()
                }})
        })
        .await??;

        Ok(data)
    }
}

enum ExportArtifact {
    Single(Bytes),
    Paged {
        total_pages: usize,
        items: Vec<(usize, Bytes)>,
    },
}

impl From<Bytes> for ExportArtifact {
    fn from(value: Bytes) -> Self {
        ExportArtifact::Single(value)
    }
}

impl From<String> for ExportArtifact {
    fn from(value: String) -> Self {
        ExportArtifact::Single(Bytes::from_string(value))
    }
}

impl From<EcoString> for ExportArtifact {
    fn from(value: EcoString) -> Self {
        ExportArtifact::Single(Bytes::from_string(value))
    }
}

trait WithPages {
    fn with_pages(self, total_pages: usize) -> ExportArtifact;
}

impl WithPages for ImageOutput<Bytes> {
    fn with_pages(self, total_pages: usize) -> ExportArtifact {
        match self {
            ImageOutput::Merged(b) => ExportArtifact::Single(b),
            ImageOutput::Paged(v) => ExportArtifact::Paged {
                total_pages,
                items: v.into_iter().map(|item| (item.page, item.value)).collect(),
            },
        }
    }
}

impl WithPages for ImageOutput<String> {
    fn with_pages(self, total_pages: usize) -> ExportArtifact {
        match self {
            ImageOutput::Merged(b) => ExportArtifact::Single(Bytes::from_string(b)),
            ImageOutput::Paged(v) => ExportArtifact::Paged {
                total_pages,
                items: v
                    .into_iter()
                    .map(|item| (item.page, Bytes::from_string(item.value)))
                    .collect(),
            },
        }
    }
}

/// User configuration for export.
#[derive(Clone, PartialEq, Eq)]
pub struct ExportUserConfig {
    /// Tinymist's default export target.
    pub export_target: ExportTarget,
    pub task: ProjectTask,
    pub count_words: bool,
    /// Whether to run the server in development mode.
    pub development: bool,
}

impl Default for ExportUserConfig {
    fn default() -> Self {
        Self {
            export_target: ExportTarget::default(),
            task: ProjectTask::ExportPdf(ExportPdfTask {
                export: ProjectExportTask {
                    when: TaskWhen::Never,
                    output: None,
                    transform: vec![],
                },
                pages: None,
                pdf_standards: vec![],
                no_pdf_tags: false,
                creation_timestamp: None,
            }),
            count_words: false,
            development: false,
        }
    }
}

fn log_err<T>(artifact: Result<T>) -> Option<T> {
    match artifact {
        Ok(v) => Some(v),
        Err(err) => {
            log::error!("{err}");
            None
        }
    }
}

fn extra_compile_for_export<D: typst::Document + Send + Sync + 'static>(
    world: &LspWorld,
) -> Result<Arc<D>> {
    let res = tokio::task::block_in_place(|| CompilationTask::<D>::execute(world));

    match res.output {
        Ok(v) => Ok(v),
        Err(e) if e.is_empty() => bail!("failed to compile: internal error"),
        Err(e) => bail!("failed to compile: {}", e[0].message),
    }
}

type FoldFuture = Pin<Box<dyn Future<Output = Option<()>> + Send>>;

#[derive(Default)]
struct FoldingState {
    running: bool,
    task: Option<(usize, FoldFuture)>,
}

#[derive(Clone, Default)]
struct FutureFolder {
    state: Arc<Mutex<FoldingState>>,
}

impl FutureFolder {
    async fn compute<'scope, OP, R: Send + 'static>(op: OP) -> Result<R>
    where
        OP: FnOnce(&Scope<'scope>) -> R + Send + 'static,
    {
        tokio::task::spawn_blocking(move || -> R { rayon::in_place_scope(op) })
            .await
            .context_ut("compute error")
    }

    #[must_use]
    fn spawn(
        &self,
        revision: usize,
        fut: impl FnOnce() -> FoldFuture,
    ) -> Option<impl Future<Output = ()> + Send + 'static> {
        let mut state = self.state.lock();
        let state = state.deref_mut();

        match &mut state.task {
            Some((prev_revision, prev)) => {
                if *prev_revision < revision {
                    *prev = fut();
                    *prev_revision = revision;
                }

                return None;
            }
            next_update => {
                *next_update = Some((revision, fut()));
            }
        }

        if state.running {
            return None;
        }

        state.running = true;

        let state = self.state.clone();
        Some(async move {
            loop {
                let fut = {
                    let mut state = state.lock();
                    let Some((_, fut)) = state.task.take() else {
                        state.running = false;
                        return;
                    };
                    fut
                };
                fut.await;
            }
        })
    }
}

fn open_external(path: &Path) {
    #[cfg(not(feature = "open"))]
    if open {
        log::warn!("open is not supported in this build, ignoring");
    }

    #[cfg(feature = "open")]
    {
        // See https://github.com/Myriad-Dreamin/tinymist/issues/837
        // Also see https://github.com/Byron/open-rs/issues/105
        #[cfg(not(target_os = "windows"))]
        let do_open = ::open::that_detached;
        #[cfg(target_os = "windows")]
        fn do_open(path: impl AsRef<std::ffi::OsStr>) -> std::io::Result<()> {
            ::open::with_detached(path, "explorer")
        }

        log::trace!("open with system default apps: {path:?}");
        do_open(path).log_error("failed to open with system default apps");
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::export::ProjectCompilation;
    use crate::project::{CompileOnceArgs, CompileSignal};
    use crate::world::base::{CompileSnapshot, WorldComputeGraph};

    #[test]
    fn test_default_never() {
        let conf = ExportUserConfig::default();
        assert!(!conf.count_words);
        assert_eq!(conf.task.when(), Some(&TaskWhen::Never));
    }

    #[test]
    fn compilation_default_never() {
        let args = CompileOnceArgs::parse_from(["tinymist", "main.typ"]);
        let verse = args
            .resolve_system()
            .expect("failed to resolve system universe");

        let snap = CompileSnapshot::from_world(verse.snapshot());

        let graph = WorldComputeGraph::new(snap);

        let needs_run =
            ProjectCompilation::preconfig_timings(&graph).expect("failed to preconfigure timings");

        assert!(!needs_run);
    }

    // todo: on demand compilation
    #[test]
    fn compilation_run_paged_diagnostics() {
        let args = CompileOnceArgs::parse_from(["tinymist", "main.typ"]);
        let verse = args
            .resolve_system()
            .expect("failed to resolve system universe");

        let mut snap = CompileSnapshot::from_world(verse.snapshot());

        snap.signal = CompileSignal {
            by_entry_update: true,
            by_fs_events: false,
            by_mem_events: false,
        };

        let graph = WorldComputeGraph::new(snap);

        let needs_run =
            ProjectCompilation::preconfig_timings(&graph).expect("failed to preconfigure timings");

        assert!(needs_run);
    }

    use chrono::{DateTime, Utc};
    use tinymist_std::time::*;

    /// Parses a UNIX timestamp according to <https://reproducible-builds.org/specs/source-date-epoch/>
    pub fn convert_source_date_epoch(seconds: i64) -> Result<DateTime<Utc>, String> {
        DateTime::from_timestamp(seconds, 0).ok_or_else(|| "timestamp out of range".to_string())
    }

    /// Parses a UNIX timestamp according to <https://reproducible-builds.org/specs/source-date-epoch/>
    pub fn convert_system_time(seconds: i64) -> Result<Time, String> {
        if seconds < 0 {
            return Err("negative timestamp since unix epoch".to_string());
        }

        Time::UNIX_EPOCH
            .checked_add(Duration::new(seconds as u64, 0))
            .ok_or_else(|| "timestamp out of range".to_string())
    }

    #[test]
    fn test_timestamp_chrono() {
        let timestamp = 1_000_000_000;
        let date_time = convert_source_date_epoch(timestamp).unwrap();
        assert_eq!(date_time.timestamp(), timestamp);
    }

    #[test]
    fn test_timestamp_system() {
        let timestamp = 1_000_000_000;
        let date_time = convert_system_time(timestamp).unwrap();
        assert_eq!(
            date_time
                .duration_since(Time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            timestamp as u64
        );
    }

    use typst::foundations::Datetime as TypstDatetime;

    fn convert_datetime_chrono(date_time: DateTime<Utc>) -> Option<TypstDatetime> {
        use chrono::{Datelike, Timelike};
        TypstDatetime::from_ymd_hms(
            date_time.year(),
            date_time.month().try_into().ok()?,
            date_time.day().try_into().ok()?,
            date_time.hour().try_into().ok()?,
            date_time.minute().try_into().ok()?,
            date_time.second().try_into().ok()?,
        )
    }

    #[test]
    fn test_timestamp_pdf() {
        let timestamp = 1_000_000_000;
        let date_time = convert_source_date_epoch(timestamp).unwrap();
        assert_eq!(date_time.timestamp(), timestamp);
        let chrono_pdf_ts = convert_datetime_chrono(date_time).unwrap();

        let timestamp = 1_000_000_000;
        let date_time = convert_system_time(timestamp).unwrap();
        let system_pdf_ts = tinymist_std::time::to_typst_time(date_time.into());
        assert_eq!(chrono_pdf_ts, system_pdf_ts);
    }
}
