//! Next generation of the export task.
//!
//! Export configuration is deliberately kept outside of [`WorldComputeGraph`].
//! The graph caches stateless compilation computations, while [`ProjectExport`]
//! is the explicit stateful orchestration layer.

use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use reflexo_typst::{Bytes, CompilerFeat, EntryReader, ExportWebSvgHtmlTask, WebSvgHtmlExport};
use reflexo_vec2svg::DefaultExportFeature;
use tinymist_std::error::prelude::*;
use tinymist_std::typst::{TypstHtmlDocument, TypstPagedDocument};
use tinymist_task::{
    output_template, DocumentQuery, ExportHtmlTask, ExportMarkdownTask, ExportTeXTask,
    ExportTimings, HtmlExport, ImageOutput, PdfExport, PngExport, SvgExport, TextExport,
};
use typlite::{Format, Typlite, TypliteFeat};
use typst::foundations::Output;
use typst::model::Document;

use crate::project::{LspCompilerFeat, ProjectTask, TaskWhen};
use crate::world::base::{
    DiagnosticsTask, ExportComputation, HtmlCompilationTask, OptionDocumentTask,
    PagedCompilationTask, WorldComputable, WorldComputeGraph,
};

/// A task that runs the baseline project diagnostics compilation.
#[derive(Clone, Copy, Default)]
pub struct ProjectCompilation;

impl ProjectCompilation {
    /// Requests the document types needed by baseline diagnostics for the
    /// current compile signal.
    pub fn run<F: CompilerFeat>(graph: &Arc<WorldComputeGraph<F>>) -> Result<bool> {
        // todo: configure run_diagnostics!
        let paged_diag = Some(TaskWhen::OnType);
        let paged_diag2 = Some(TaskWhen::Script);
        let html_diag = Some(TaskWhen::Never);

        let check = |timing: Option<TaskWhen>| {
            ExportTimings::needs_run::<F, TypstPagedDocument>(&graph.snap, timing.as_ref(), None)
                .unwrap_or(true)
        };

        let compile_paged = [paged_diag, paged_diag2].into_iter().any(check);
        let compile_html = [html_diag].into_iter().any(check);

        if compile_paged {
            let _ = graph.compute::<PagedCompilationTask>()?;
        }
        if compile_html {
            let _ = graph.compute::<HtmlCompilationTask>()?;
        }

        Ok(compile_paged || compile_html)
    }
}

impl<F: CompilerFeat> WorldComputable<F> for ProjectCompilation {
    type Output = Self;

    fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self> {
        Self::run(graph)?;
        Ok(Self)
    }
}

/// A stateful aggregate that runs every registered project export task.
#[derive(Default)]
pub struct ProjectExport {
    tasks: Vec<ProjectTask>,
}

impl ProjectExport {
    /// Creates an aggregate from registered export tasks.
    pub fn new(tasks: impl IntoIterator<Item = ProjectTask>) -> Self {
        Self {
            tasks: tasks.into_iter().collect(),
        }
    }

    /// Registers another export task.
    pub fn register(&mut self, task: ProjectTask) {
        self.tasks.push(task);
    }

    /// Returns all registered export tasks in dispatch order.
    pub fn registered_tasks(&self) -> &[ProjectTask] {
        &self.tasks
    }

    /// Runs baseline compilation, all registered exports, and then collects
    /// diagnostics from the compilation targets that were actually requested.
    pub fn run(
        &self,
        graph: &Arc<WorldComputeGraph<LspCompilerFeat>>,
    ) -> Result<Arc<DiagnosticsTask>> {
        let _ = graph.compute::<ProjectCompilation>()?;

        for task in &self.tasks {
            Self::run_registered(graph, task)?;
        }

        graph.shared_diagnostics()
    }

    /// This is the intentionally explicit export-task registry. Adding a new
    /// project task requires choosing its compilation dependency here.
    fn run_registered(
        graph: &Arc<WorldComputeGraph<LspCompilerFeat>>,
        task: &ProjectTask,
    ) -> Result<()> {
        use ProjectTask::*;

        let Some(path) = output_path(graph, task) else {
            return Ok(());
        };
        let when = task.when();

        match task {
            Preview(..) => Ok(()),
            ExportPdf(config) => {
                write_single(&path, PagedExport::<PdfExport>::run(graph, when, config)?)
            }
            ExportPng(config) => {
                let path = image_output_path(path, config.page_number_template.as_deref());
                let output = PagedExport::<PngExport>::run(graph, when, config)?;
                write_image(graph, &path, output)
            }
            ExportSvg(config) => {
                let path = image_output_path(path, config.page_number_template.as_deref());
                let output = PagedExport::<SvgExport>::run(graph, when, config)?;
                write_image(graph, &path, output)
            }
            ExportHtml(config) => write_single(
                &path,
                HtmlProjectExport::run(graph, when, config)?.map(Bytes::from_string),
            ),
            ExportBundle(..) => {
                bail!("bundle export is not implemented by export2")
            }
            ExportSvgHtml(..) => write_single(
                &path,
                SvgHtmlProjectExport::run(graph, when, &ExportWebSvgHtmlTask::default())?
                    .map(Bytes::from_string),
            ),
            ExportMd(config) => write_single(
                &path,
                TypliteMdExport::run(graph, when, config)?.map(Bytes::from_string),
            ),
            ExportTeX(config) => write_single(
                &path,
                TypliteTeXExport::run(graph, when, config)?.map(Bytes::from_string),
            ),
            ExportText(config) => write_single(
                &path,
                PagedExport::<TextExport>::run(graph, when, config)?.map(Bytes::from_string),
            ),
            Query(config) => {
                let output = PagedExport::<DocumentQuery>::run(graph, when, config)?
                    .map(|output| {
                        output
                            .map(Bytes::from_string)
                            .map_err(|errors| anyhow::anyhow!("query export failed: {errors:?}"))
                    })
                    .transpose()?;
                write_single(&path, output)
            }
        }
    }
}

/// A small export task that depends on paged compilation and an existing
/// format implementation.
struct PagedExport<T>(PhantomData<T>);

impl<T> PagedExport<T> {
    fn run<F: CompilerFeat>(
        graph: &Arc<WorldComputeGraph<F>>,
        when: Option<&TaskWhen>,
        config: &<T as ExportComputation<F, TypstPagedDocument>>::Config,
    ) -> Result<Option<<T as ExportComputation<F, TypstPagedDocument>>::Output>>
    where
        T: ExportComputation<F, TypstPagedDocument>,
    {
        DocumentExport::<TypstPagedDocument, T>::run::<F>(graph, when, config)
    }
}

/// A small export task that depends on HTML compilation and the existing HTML
/// export implementation.
struct HtmlProjectExport;

impl HtmlProjectExport {
    fn run<F: CompilerFeat>(
        graph: &Arc<WorldComputeGraph<F>>,
        when: Option<&TaskWhen>,
        config: &ExportHtmlTask,
    ) -> Result<Option<String>>
    where
        HtmlExport:
            ExportComputation<F, TypstHtmlDocument, Config = ExportHtmlTask, Output = String>,
    {
        DocumentExport::<TypstHtmlDocument, HtmlExport>::run::<F>(graph, when, config)
    }
}

/// A small export task that depends on paged compilation and the existing SVG
/// HTML export implementation.
struct SvgHtmlProjectExport;

impl SvgHtmlProjectExport {
    fn run<F: CompilerFeat>(
        graph: &Arc<WorldComputeGraph<F>>,
        when: Option<&TaskWhen>,
        config: &ExportWebSvgHtmlTask,
    ) -> Result<Option<String>>
    where
        WebSvgHtmlExport<DefaultExportFeature>: ExportComputation<
            F,
            TypstPagedDocument,
            Config = ExportWebSvgHtmlTask,
            Output = String,
        >,
    {
        DocumentExport::<TypstPagedDocument, WebSvgHtmlExport<DefaultExportFeature>>::run::<F>(
            graph, when, config,
        )
    }
}

/// A typed small export task that performs timing checks and delegates to an
/// existing stateless export implementation.
struct DocumentExport<D, T>(PhantomData<(D, T)>);

impl<D, T> DocumentExport<D, T>
where
    D: Document + Output + Send + Sync + 'static,
{
    fn run<F: CompilerFeat>(
        graph: &Arc<WorldComputeGraph<F>>,
        when: Option<&TaskWhen>,
        config: &<T as ExportComputation<F, D>>::Config,
    ) -> Result<Option<<T as ExportComputation<F, D>>::Output>>
    where
        T: ExportComputation<F, D>,
    {
        let Some(doc) = required_document::<F, D>(graph, when)? else {
            return Ok(None);
        };

        Ok(Some(T::run(graph, &doc, config)?))
    }
}

fn required_document<F, D>(
    graph: &Arc<WorldComputeGraph<F>>,
    when: Option<&TaskWhen>,
) -> Result<Option<Arc<D>>>
where
    F: CompilerFeat,
    D: Document + Output + Send + Sync + 'static,
{
    let before = ExportTimings::needs_run::<F, D>(&graph.snap, when, None);
    if before == Some(false) {
        return Ok(None);
    }

    let doc = graph.compute::<OptionDocumentTask<D>>()?;
    let Some(doc) = doc.as_ref() else {
        return Ok(None);
    };

    let after = ExportTimings::needs_run(&graph.snap, when, Some(doc.as_ref())).unwrap_or(true);
    Ok(after.then(|| doc.clone()))
}

fn output_path<F: CompilerFeat>(
    graph: &WorldComputeGraph<F>,
    task: &ProjectTask,
) -> Option<PathBuf> {
    task.as_export()?
        .output
        .as_ref()?
        .substitute(&graph.snap.world.entry_state())
        .map(|path| path.to_path_buf())
}

fn image_output_path(path: PathBuf, template: Option<&str>) -> PathBuf {
    match template {
        Some(template) => path.with_file_name(template),
        None => path,
    }
}

fn write_single(path: &Path, output: Option<Bytes>) -> Result<()> {
    let Some(output) = output else {
        return Ok(());
    };

    write_file(path, &output)
}

fn write_image<F, T>(
    graph: &Arc<WorldComputeGraph<F>>,
    path: &Path,
    output: Option<ImageOutput<T>>,
) -> Result<()>
where
    F: CompilerFeat,
    T: IntoExportBytes,
{
    let Some(output) = output else {
        return Ok(());
    };

    match output {
        ImageOutput::Merged(output) => write_file(path, &output.into_export_bytes()),
        ImageOutput::Paged(outputs) => {
            if outputs.len() > 1
                && !output_template::has_indexable_template(path.to_string_lossy().as_ref())
            {
                bail!("multiple-page export requires a page-number template: {path:?}");
            }

            let total_pages = graph
                .shared_compile()?
                .map(|doc| doc.pages().len())
                .unwrap_or(outputs.len());
            for output in outputs {
                let rendered = output_template::format(
                    path.to_string_lossy().as_ref(),
                    output.page + 1,
                    total_pages,
                );
                write_file(Path::new(&rendered), &output.value.into_export_bytes())?;
            }
            Ok(())
        }
    }
}

fn write_file(path: &Path, output: &Bytes) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).context("failed to create export directory")?;
    }
    std::fs::write(path, output).context("failed to write output")
}

trait IntoExportBytes {
    fn into_export_bytes(self) -> Bytes;
}

impl IntoExportBytes for Bytes {
    fn into_export_bytes(self) -> Bytes {
        self
    }
}

impl IntoExportBytes for String {
    fn into_export_bytes(self) -> Bytes {
        Bytes::from_string(self)
    }
}

/// A task that exports the document to a specific format by Typlite.
pub struct TypliteExport<const FORMAT: char>;

const fn typlite_format(format: char) -> Format {
    match format {
        'm' => Format::Md,
        'x' => Format::LaTeX,
        _ => panic!("unsupported format for TypliteExport"),
    }
}

const fn typlite_name(format: char) -> &'static str {
    match format {
        'm' => "Markdown",
        'x' => "LaTeX",
        _ => panic!("unsupported format for TypliteExport"),
    }
}

impl<const FORMAT: char> TypliteExport<FORMAT> {
    fn run(
        graph: &Arc<WorldComputeGraph<LspCompilerFeat>>,
        when: Option<&TaskWhen>,
        processor: Option<String>,
        assets_path: Option<PathBuf>,
    ) -> Result<Option<String>> {
        if required_document::<LspCompilerFeat, TypstPagedDocument>(graph, when)?.is_none() {
            return Ok(None);
        }

        let conv = Typlite::new(Arc::new(graph.snap.world.clone()))
            .with_format(typlite_format(FORMAT))
            .with_feature(TypliteFeat {
                processor,
                assets_path,
                ..Default::default()
            })
            .convert()
            .map_err(|error| {
                anyhow::anyhow!("failed to convert to {}: {error}", typlite_name(FORMAT))
            })?;

        Ok(Some(conv.to_string()))
    }
}

/// A task that exports the document to Markdown.
pub struct TypliteMdExport;

impl TypliteMdExport {
    fn run(
        graph: &Arc<WorldComputeGraph<LspCompilerFeat>>,
        when: Option<&TaskWhen>,
        config: &ExportMarkdownTask,
    ) -> Result<Option<String>> {
        TypliteExport::<'m'>::run(
            graph,
            when,
            config.processor.clone(),
            config.assets_path.clone(),
        )
    }
}

/// A task that exports the document to LaTeX.
pub struct TypliteTeXExport;

impl TypliteTeXExport {
    fn run(
        graph: &Arc<WorldComputeGraph<LspCompilerFeat>>,
        when: Option<&TaskWhen>,
        config: &ExportTeXTask,
    ) -> Result<Option<String>> {
        TypliteExport::<'x'>::run(
            graph,
            when,
            config.processor.clone(),
            config.assets_path.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use clap::Parser;
    use tempfile::TempDir;
    use tinymist_task::{
        ExportBundleTask, ExportHtmlTask, ExportMarkdownTask, ExportPdfTask, ExportPngTask,
        ExportSvgTask, ExportTask, ExportTeXTask, ExportTextTask, PathPattern, PreviewTask,
        QueryTask,
    };

    use super::*;
    use crate::project::{CompileOnceArgs, CompileSignal, WorldProvider};
    use crate::world::base::{BundleCompilationTask, CompileSnapshot};

    fn test_graph(
        source: &str,
        signal: CompileSignal,
    ) -> (TempDir, Arc<WorldComputeGraph<LspCompilerFeat>>) {
        let temp = tempfile::tempdir().expect("temporary directory");
        let input = temp.path().join("main.typ");
        fs::write(&input, source).expect("test source");

        let args = CompileOnceArgs::parse_from([
            "tinymist",
            input.to_str().expect("UTF-8 temporary path"),
        ]);
        let verse = args.resolve().expect("failed to resolve LSP universe");
        let mut snap = CompileSnapshot::from_world(verse.snapshot());
        snap.signal = signal;

        (temp, WorldComputeGraph::new(snap))
    }

    fn export_task(path: &Path, when: TaskWhen) -> ExportTask {
        ExportTask {
            when,
            output: Some(PathPattern::new(path.to_str().expect("UTF-8 output path"))),
            transform: vec![],
        }
    }

    fn pdf_task(path: &Path, when: TaskWhen) -> ProjectTask {
        let config = ExportPdfTask {
            export: export_task(path, when),
            ..ExportPdfTask::default()
        };
        ProjectTask::ExportPdf(config)
    }

    fn html_task(path: &Path, when: TaskWhen) -> ProjectTask {
        let config = ExportHtmlTask {
            export: export_task(path, when),
        };
        ProjectTask::ExportHtml(config)
    }

    #[test]
    fn compilation_is_cached_without_flags_and_diagnostics_are_selective() {
        let (_temp, graph) = test_graph("#let", CompileSignal::default());
        let diagnostics = graph.shared_diagnostics().expect("initial diagnostics");
        assert_eq!(diagnostics.error_cnt(), 0);

        let first = graph
            .compute::<PagedCompilationTask>()
            .expect("paged compilation");
        let second = graph
            .compute::<PagedCompilationTask>()
            .expect("cached paged compilation");
        assert!(Arc::ptr_eq(&first, &second));

        let diagnostics = graph.shared_diagnostics().expect("diagnostics");
        assert!(diagnostics.error_cnt() > 0);
        assert!(graph.get::<HtmlCompilationTask>().is_none());
        assert!(graph.get::<BundleCompilationTask>().is_none());
    }

    #[test]
    fn aggregate_skips_unregistered_and_inactive_exports() {
        let signal = CompileSignal {
            by_fs_events: true,
            ..CompileSignal::default()
        };
        let (temp, graph) = test_graph("= Hello", signal);

        ProjectExport::default()
            .run(&graph)
            .expect("empty aggregate export");
        assert!(graph.get::<PagedCompilationTask>().is_none());
        assert!(graph.get::<HtmlCompilationTask>().is_none());

        let output = temp.path().join("inactive.pdf");
        ProjectExport::new([pdf_task(&output, TaskWhen::Never)])
            .run(&graph)
            .expect("inactive aggregate export");
        assert!(graph.get::<PagedCompilationTask>().is_none());
        assert!(!output.exists());
    }

    #[test]
    fn aggregate_dispatches_all_supported_exports_to_their_compilations() {
        let signal = CompileSignal {
            by_fs_events: true,
            ..CompileSignal::default()
        };
        let (temp, graph) = test_graph("= Hello", signal);
        let pdf = temp.path().join("active.pdf");
        let png = temp.path().join("active.png");
        let svg = temp.path().join("active.svg");
        let html = temp.path().join("active.html");
        let svg_html = temp.path().join("active-svg.html");
        let markdown = temp.path().join("active.md");
        let tex = temp.path().join("active.tex");
        let text = temp.path().join("active.txt");
        let query = temp.path().join("active.json");

        let mut export = ProjectExport::default();
        export.register(ProjectTask::Preview(PreviewTask {
            when: TaskWhen::OnSave,
        }));
        export.register(pdf_task(&pdf, TaskWhen::OnSave));
        export.register(ProjectTask::ExportPng(ExportPngTask {
            export: export_task(&png, TaskWhen::OnSave),
            pages: None,
            page_number_template: None,
            merge: None,
            ppi: 144.0.try_into().expect("valid PPI"),
            fill: None,
        }));
        export.register(ProjectTask::ExportSvg(ExportSvgTask {
            export: export_task(&svg, TaskWhen::OnSave),
            ..ExportSvgTask::default()
        }));
        export.register(html_task(&html, TaskWhen::OnSave));
        export.register(ProjectTask::ExportSvgHtml(ExportHtmlTask {
            export: export_task(&svg_html, TaskWhen::OnSave),
        }));
        export.register(ProjectTask::ExportMd(ExportMarkdownTask {
            export: export_task(&markdown, TaskWhen::OnSave),
            ..ExportMarkdownTask::default()
        }));
        export.register(ProjectTask::ExportTeX(ExportTeXTask {
            export: export_task(&tex, TaskWhen::OnSave),
            ..ExportTeXTask::default()
        }));
        export.register(ProjectTask::ExportText(ExportTextTask {
            export: export_task(&text, TaskWhen::OnSave),
        }));
        export.register(ProjectTask::Query(QueryTask {
            export: export_task(&query, TaskWhen::OnSave),
            format: "json".into(),
            output_extension: None,
            selector: "heading".into(),
            field: None,
            one: false,
        }));

        assert_eq!(export.registered_tasks().len(), 10);
        let diagnostics = export.run(&graph).expect("active aggregate export");

        assert_eq!(diagnostics.error_cnt(), 0);
        assert!(graph.get::<PagedCompilationTask>().is_some());
        assert!(graph.get::<HtmlCompilationTask>().is_some());
        assert!(graph.get::<BundleCompilationTask>().is_none());
        for output in [pdf, png, svg, html, svg_html, markdown, tex, text, query] {
            assert!(output.exists(), "missing export output: {output:?}");
            assert!(
                fs::metadata(&output).expect("export metadata").len() > 0,
                "empty export output: {output:?}"
            );
        }
    }

    #[test]
    fn aggregate_rejects_bundle_until_it_has_a_reusable_export_impl() {
        let signal = CompileSignal {
            by_fs_events: true,
            ..CompileSignal::default()
        };
        let (temp, graph) = test_graph("= Hello", signal);
        let output = temp.path().join("bundle");
        let task = ProjectTask::ExportBundle(ExportBundleTask {
            export: export_task(&output, TaskWhen::OnSave),
            pages: None,
            pdf_standards: vec![],
            no_pdf_tags: false,
            creation_timestamp: None,
            ppi: 144.0.try_into().expect("valid PPI"),
        });

        let error = match ProjectExport::new([task]).run(&graph) {
            Ok(_) => panic!("bundle export must remain explicit"),
            Err(error) => error,
        };

        assert!(error
            .to_string()
            .contains("bundle export is not implemented"));
        assert!(graph.get::<BundleCompilationTask>().is_none());
        assert!(!output.exists());
    }
}
