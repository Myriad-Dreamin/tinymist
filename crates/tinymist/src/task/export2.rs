#![allow(missing_docs)]

use std::str::FromStr;
use std::sync::Arc;

use reflexo_typst::{Bytes, EntryReader, TypstDatetime};
use tinymist_project::{
    convert_source_date_epoch, CompileSnapshot, ExportSvgTask, LspCompilerFeat, TaskWhen,
};
use tinymist_std::error::prelude::*;
use tinymist_std::typst::{TypstDocument, TypstHtmlDocument, TypstPagedDocument};
use typlite::Typlite;
use typst::diag::{SourceResult, Warned};
use typst::ecow::EcoVec;
use typst::visualize::Color;
use typst_pdf::{PdfOptions, Timestamp};

use super::export::*;
use crate::project::{
    ExportHtmlTask, ExportMarkdownTask, ExportPdfTask, ExportPngTask, ExportTextTask, ProjectTask,
};
use crate::tool::text::FullTextDigest;
use crate::world::base::{WorldComputable, WorldComputeGraph};

pub struct TaskConfig<T>(T);

impl<T: Send + Sync + 'static> WorldComputable<LspCompilerFeat> for TaskConfig<T> {
    fn compute(_graph: &Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<Self> {
        let id = std::any::type_name::<T>();
        panic!("{id:?} must be provided before computation");
    }
}

type TaskFlag<T> = TaskConfig<TaskFlagBase<T>>;
struct TaskFlagBase<T> {
    enabled: bool,
    _phantom: std::marker::PhantomData<T>,
}

type PagedCompilation = Compilation<TypstPagedDocument>;
type HtmlCompilation = Compilation<TypstHtmlDocument>;

pub struct Compilation<D>(Option<Warned<SourceResult<Arc<D>>>>);

impl<D> WorldComputable<LspCompilerFeat> for Compilation<D>
where
    D: typst::Document + Send + Sync + 'static,
{
    fn compute(graph: &Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<Self> {
        let enabled = graph.must_get::<TaskFlag<Compilation<D>>>()?.0.enabled;

        Ok(Self(enabled.then(|| {
            let compiled = typst::compile::<D>(&graph.snap.world);
            Warned {
                output: compiled.output.map(Arc::new),
                warnings: compiled.warnings,
            }
        })))
    }
}

pub struct OptionDocument<D>(Option<Arc<D>>);

impl<D> WorldComputable<LspCompilerFeat> for OptionDocument<D>
where
    D: typst::Document + Send + Sync + 'static,
{
    fn compute(graph: &Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<Self> {
        let doc = graph.compute::<Compilation<D>>()?;
        let compiled = doc.0.as_ref().and_then(|warned| warned.output.clone().ok());

        Ok(Self(compiled))
    }
}

pub trait ExportComputation<D> {
    type Output;
    type Config: Send + Sync + 'static;

    fn needs_run(
        graph: &Arc<WorldComputeGraph<LspCompilerFeat>>,
        doc: Option<&D>,
        config: &Self::Config,
    ) -> bool;

    fn run(doc: &Arc<D>, config: &Self::Config) -> Result<Self::Output>;
}

impl<D> OptionDocument<D>
where
    D: typst::Document + Send + Sync + 'static,
{
    fn needs_run<C: Send + Sync + 'static>(
        graph: &Arc<WorldComputeGraph<LspCompilerFeat>>,
        f: impl FnOnce(&Arc<WorldComputeGraph<LspCompilerFeat>>, Option<&D>, &C) -> bool,
    ) -> Result<bool> {
        let Some(config) = graph.get::<TaskConfig<C>>().transpose()? else {
            return Ok(false);
        };

        let doc = graph.compute::<OptionDocument<D>>()?;
        Ok(f(graph, doc.0.as_deref(), &config.0))
    }

    pub fn run_export<T: ExportComputation<D>>(
        graph: &Arc<WorldComputeGraph<LspCompilerFeat>>,
    ) -> Result<Option<T::Output>> {
        if !OptionDocument::needs_run(graph, T::needs_run)? {
            return Ok(None);
        }

        let doc = graph.compute::<OptionDocument<D>>()?.0.clone();
        let config = graph.get::<TaskConfig<T::Config>>().transpose()?;

        let result = doc
            .zip(config)
            .map(|(doc, config)| T::run(&doc, &config.0))
            .transpose()?;

        Ok(result)
    }
}

struct CompilationDiagnostics {
    errors: Option<EcoVec<typst::diag::SourceDiagnostic>>,
    warnings: Option<EcoVec<typst::diag::SourceDiagnostic>>,
}

impl CompilationDiagnostics {
    fn from_result<T>(result: Option<Warned<SourceResult<T>>>) -> Self {
        let errors = result
            .as_ref()
            .and_then(|r| r.output.as_ref().map_err(|e| e.clone()).err());
        let warnings = result.as_ref().map(|r| r.warnings.clone());

        Self { errors, warnings }
    }
}

pub struct Diagnostics {
    paged: CompilationDiagnostics,
    html: CompilationDiagnostics,
}

impl WorldComputable<LspCompilerFeat> for Diagnostics {
    fn compute(graph: &Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<Self> {
        let paged = graph.compute::<PagedCompilation>()?.0.clone();
        let html = graph.compute::<HtmlCompilation>()?.0.clone();

        Ok(Self {
            paged: CompilationDiagnostics::from_result(paged),
            html: CompilationDiagnostics::from_result(html),
        })
    }
}

impl Diagnostics {
    pub fn diagnostics(&self) -> impl Iterator<Item = &typst::diag::SourceDiagnostic> {
        self.paged
            .errors
            .iter()
            .chain(self.paged.warnings.iter())
            .chain(self.html.errors.iter())
            .chain(self.html.warnings.iter())
            .flatten()
    }
}

struct PdfFlag;
struct SvgFlag;
struct PngFlag;
struct HtmlFlag;
struct MarkdownFlag;
struct TextFlag;

type ErasedVecExport<E> = ErasedExport<SourceResult<Vec<u8>>, E>;
type ErasedStrExport<E> = ErasedExport<SourceResult<String>, E>;
type ErasedPdfExport = ErasedVecExport<PdfFlag>;
type ErasedSvgExport = ErasedStrExport<SvgFlag>;
type ErasedPngExport = ErasedVecExport<PngFlag>;
type ErasedHtmlExport = ErasedStrExport<HtmlFlag>;
type ErasedMarkdownExport = ErasedStrExport<MarkdownFlag>;
type ErasedTextExport = ErasedStrExport<TextFlag>;

pub struct ErasedExport<T, E> {
    result: Option<T>,
    _phantom: std::marker::PhantomData<E>,
}

#[allow(clippy::type_complexity)]
struct ErasedExportImpl<T, E> {
    f: Arc<
        dyn Fn(&Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<ErasedExport<T, E>>
            + Send
            + Sync,
    >,
    _phantom: std::marker::PhantomData<(T, E)>,
}

impl<T: Send + Sync + 'static, E: Send + Sync + 'static> ErasedExport<T, E> {
    pub fn provide_raw(
        graph: &Arc<WorldComputeGraph<LspCompilerFeat>>,
        f: impl Fn(&Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<Option<T>>
            + Send
            + Sync
            + 'static,
    ) -> Result<()> {
        let _ = graph.provide::<TaskConfig<ErasedExportImpl<T, E>>>(Ok(Arc::new({
            TaskConfig(ErasedExportImpl {
                f: Arc::new(move |graph| {
                    let result = f(graph)?;
                    Ok(ErasedExport {
                        result,
                        _phantom: std::marker::PhantomData,
                    })
                }),
                _phantom: std::marker::PhantomData,
            })
        })));
        Ok(())
    }

    pub fn provide<D, C>(graph: &Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<()>
    where
        D: typst::Document + Send + Sync + 'static,
        C: WorldComputable<LspCompilerFeat> + ExportComputation<D, Output = T>,
    {
        Self::provide_raw(graph, OptionDocument::run_export::<C>)
    }
}

impl<T: Send + Sync + 'static, E: Send + Sync + 'static> WorldComputable<LspCompilerFeat>
    for ErasedExport<T, E>
{
    fn compute(graph: &Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<Self> {
        let f = graph.must_get::<TaskConfig<ErasedExportImpl<T, E>>>()?;
        (f.0.f)(graph)
    }
}

pub struct ProjectExport;

impl ProjectExport {
    pub fn provide(graph: &Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<()> {
        ErasedExport::<_, PdfFlag>::provide::<TypstPagedDocument, PdfExport>(graph)?;
        ErasedExport::<_, SvgFlag>::provide::<TypstPagedDocument, SvgExport>(graph)?;
        ErasedExport::<_, PngFlag>::provide::<TypstPagedDocument, PngExport>(graph)?;
        ErasedExport::<_, HtmlFlag>::provide::<TypstHtmlDocument, HtmlExport>(graph)?;
        ErasedExport::<_, MarkdownFlag>::provide_raw(graph, TypliteMarkdownExport::run)?;
        ErasedExport::<_, TextFlag>::provide::<TypstPagedDocument, TextExport>(graph)?;
        Ok(())
    }
}

pub struct ProjectCompilation;

impl ProjectCompilation {
    fn needs_run<D: typst::Document>(
        snap: &CompileSnapshot<LspCompilerFeat>,
        timing: Option<TaskWhen>,
        docs: Option<&D>,
    ) -> Option<bool> {
        let s = snap.signal;
        let when = timing.unwrap_or(TaskWhen::Never);
        if !matches!(when, TaskWhen::Never) && s.by_entry_update {
            return Some(true);
        }

        match when {
            TaskWhen::Never => Some(false),
            TaskWhen::OnType => Some(s.by_mem_events),
            TaskWhen::OnSave => Some(s.by_fs_events),
            TaskWhen::OnDocumentHasTitle if s.by_fs_events => {
                docs.map(|doc| doc.info().title.is_some())
            }
            TaskWhen::OnDocumentHasTitle => Some(false),
        }
    }

    pub fn preconfig_timings(graph: &Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<bool> {
        // todo: configure run_diagnostics!
        let run_paged_diagnostics = Some(TaskWhen::OnType);
        let run_html_diagnostics = Some(TaskWhen::Never);

        let pdf_timing: Option<TaskWhen> = graph
            .get::<TaskConfig<<PdfExport as ExportComputation<_>>::Config>>()
            .transpose()?
            .map(|config| config.0.export.when);
        let svg_timing: Option<TaskWhen> = graph
            .get::<TaskConfig<<SvgExport as ExportComputation<_>>::Config>>()
            .transpose()?
            .map(|config| config.0.export.when);
        let png_timing: Option<TaskWhen> = graph
            .get::<TaskConfig<<PngExport as ExportComputation<_>>::Config>>()
            .transpose()?
            .map(|config| config.0.export.when);
        let html_timing: Option<TaskWhen> = graph
            .get::<TaskConfig<<HtmlExport as ExportComputation<_>>::Config>>()
            .transpose()?
            .map(|config| config.0.export.when);
        let markdown_timing: Option<TaskWhen> = graph
            .get::<TaskConfig<ExportMarkdownTask>>()
            .transpose()?
            .map(|config| config.0.export.when);
        let text_timing: Option<TaskWhen> = graph
            .get::<TaskConfig<<TextExport as ExportComputation<_>>::Config>>()
            .transpose()?
            .map(|config| config.0.export.when);

        let doc = None::<TypstPagedDocument>.as_ref();
        let check_timing = |timing| Self::needs_run(&graph.snap, timing, doc).unwrap_or(true);

        let compile_paged = check_timing(run_paged_diagnostics)
            || check_timing(pdf_timing)
            || check_timing(svg_timing)
            || check_timing(png_timing)
            || check_timing(text_timing)
            || check_timing(markdown_timing);
        let compile_html = check_timing(run_html_diagnostics) || check_timing(html_timing);

        let _ =
            graph.provide::<TaskFlag<PagedCompilation>>(Ok(Arc::new(TaskConfig(TaskFlagBase {
                enabled: compile_paged,
                _phantom: Default::default(),
            }))));
        let _ =
            graph.provide::<TaskFlag<HtmlCompilation>>(Ok(Arc::new(TaskConfig(TaskFlagBase {
                enabled: compile_html,
                _phantom: Default::default(),
            }))));

        Ok(compile_paged || compile_html)
    }
}

impl WorldComputable<LspCompilerFeat> for ProjectCompilation {
    fn compute(graph: &Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<Self> {
        Self::preconfig_timings(graph)?;
        Diagnostics::compute(graph)?;
        Ok(Self)
    }
}

impl WorldComputable<LspCompilerFeat> for ProjectExport {
    fn compute(graph: &Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<Self> {
        let config = graph.must_get::<TaskConfig<ProjectTask>>()?;
        let output_path = config.0.as_export().and_then(|e| {
            e.output
                .as_ref()
                .and_then(|o| o.substitute(&graph.snap.world.entry_state()))
        });

        let output = || -> Result<Option<SourceResult<Vec<u8>>>> {
            Ok(match &config.0 {
                ProjectTask::Preview(..) => todo!(),
                ProjectTask::ExportPdf(..) => graph.compute::<ErasedPdfExport>()?.result.clone(),
                ProjectTask::ExportPng(..) => graph.compute::<ErasedPngExport>()?.result.clone(),
                ProjectTask::ExportSvg(..) => {
                    let svg = graph.compute::<ErasedSvgExport>()?.result.clone();
                    svg.map(|s| s.map(|s| s.into_bytes()))
                }
                ProjectTask::ExportHtml(..) => {
                    let html = graph.compute::<ErasedHtmlExport>()?.result.clone();
                    html.map(|s| s.map(|s| s.into_bytes()))
                }
                ProjectTask::ExportMarkdown(..) => {
                    let markdown = graph.compute::<ErasedMarkdownExport>()?.result.clone();
                    markdown.map(|s| s.map(|s| s.into_bytes()))
                }
                ProjectTask::ExportText(..) => {
                    let text = graph.compute::<ErasedTextExport>()?.result.clone();
                    text.map(|s| s.map(|s| s.into_bytes()))
                }
                ProjectTask::Query(..) => todo!(),
            })
        };

        if let Some(path) = output_path {
            let output = output()?;
            // todo: don't ignore export source diagnostics
            if let Some(Ok(output)) = output {
                std::fs::write(path, output).context("failed to write output")?;
            }
        }

        Ok(Self {})
    }
}

pub struct PdfExport(pub Option<SourceResult<Bytes>>);

impl ExportComputation<TypstPagedDocument> for PdfExport {
    type Output = SourceResult<Bytes>;
    type Config = ExportPdfTask;

    fn needs_run(
        graph: &Arc<WorldComputeGraph<LspCompilerFeat>>,
        doc: Option<&TypstPagedDocument>,
        config: &Self::Config,
    ) -> bool {
        let timing = config.export.when;
        ProjectCompilation::needs_run(&graph.snap, Some(timing), doc).unwrap_or_default()
    }

    fn run(doc: &Arc<TypstPagedDocument>, config: &ExportPdfTask) -> Result<SourceResult<Bytes>> {
        // todo: timestamp world.now()
        let creation_timestamp = config
            .creation_timestamp
            .map(convert_source_date_epoch)
            .transpose()
            .context_ut("parse pdf creation timestamp")?
            .unwrap_or_else(chrono::Utc::now);

        // todo: Some(pdf_uri.as_str())

        let bytes = typst_pdf::pdf(
            doc,
            &PdfOptions {
                timestamp: convert_datetime(creation_timestamp),
                ..Default::default()
            },
        );

        Ok(bytes.map(Bytes::new))
    }
}

impl WorldComputable<LspCompilerFeat> for PdfExport {
    fn compute(graph: &Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<Self> {
        Ok(Self(OptionDocument::run_export::<Self>(graph)?))
    }
}

pub struct SvgExport(pub Option<SourceResult<String>>);

impl ExportComputation<TypstPagedDocument> for SvgExport {
    type Output = SourceResult<String>;
    type Config = ExportSvgTask;

    fn needs_run(
        graph: &Arc<WorldComputeGraph<LspCompilerFeat>>,
        doc: Option<&TypstPagedDocument>,
        config: &Self::Config,
    ) -> bool {
        let timing = config.export.when;
        ProjectCompilation::needs_run(&graph.snap, Some(timing), doc).unwrap_or_default()
    }

    fn run(doc: &Arc<TypstPagedDocument>, config: &ExportSvgTask) -> Result<SourceResult<String>> {
        let (is_first, merged_gap) = get_page_selection(&config.export)?;

        let first_page = doc.pages.first();

        Ok(Ok(if is_first {
            if let Some(first_page) = first_page {
                typst_svg::svg(first_page)
            } else {
                typst_svg::svg_merged(doc, merged_gap)
            }
        } else {
            typst_svg::svg_merged(doc, merged_gap)
        }))
    }
}

impl WorldComputable<LspCompilerFeat> for SvgExport {
    fn compute(graph: &Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<Self> {
        Ok(Self(OptionDocument::run_export::<Self>(graph)?))
    }
}

pub struct PngExport(pub Option<SourceResult<Bytes>>);

impl ExportComputation<TypstPagedDocument> for PngExport {
    type Output = SourceResult<Bytes>;
    type Config = ExportPngTask;

    fn needs_run(
        graph: &Arc<WorldComputeGraph<LspCompilerFeat>>,
        doc: Option<&TypstPagedDocument>,
        config: &Self::Config,
    ) -> bool {
        let timing = config.export.when;
        ProjectCompilation::needs_run(&graph.snap, Some(timing), doc).unwrap_or_default()
    }

    fn run(doc: &Arc<TypstPagedDocument>, config: &ExportPngTask) -> Result<SourceResult<Bytes>> {
        let ppi = config.ppi.to_f32();
        if ppi <= 1e-6 {
            tinymist_std::bail!("invalid ppi: {ppi}");
        }

        let fill = if let Some(fill) = &config.fill {
            parse_color(fill.clone()).map_err(|err| anyhow::anyhow!("invalid fill ({err})"))?
        } else {
            Color::WHITE
        };

        let (is_first, merged_gap) = get_page_selection(&config.export)?;

        let ppp = ppi / 72.;
        let pixmap = if is_first {
            if let Some(first_page) = doc.pages.first() {
                typst_render::render(first_page, ppp)
            } else {
                typst_render::render_merged(doc, ppp, merged_gap, Some(fill))
            }
        } else {
            typst_render::render_merged(doc, ppp, merged_gap, Some(fill))
        };

        pixmap
            .encode_png()
            .map(Bytes::new)
            .context_ut("failed to encode PNG")
            .map(Ok)
    }
}

impl WorldComputable<LspCompilerFeat> for PngExport {
    fn compute(graph: &Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<Self> {
        Ok(Self(OptionDocument::run_export::<Self>(graph)?))
    }
}

pub struct HtmlExport(pub Option<SourceResult<String>>);

impl ExportComputation<TypstHtmlDocument> for HtmlExport {
    type Output = SourceResult<String>;
    type Config = ExportHtmlTask;

    fn needs_run(
        graph: &Arc<WorldComputeGraph<LspCompilerFeat>>,
        doc: Option<&TypstHtmlDocument>,
        config: &Self::Config,
    ) -> bool {
        let timing = config.export.when;
        ProjectCompilation::needs_run(&graph.snap, Some(timing), doc).unwrap_or_default()
    }

    fn run(doc: &Arc<TypstHtmlDocument>, _config: &ExportHtmlTask) -> Result<SourceResult<String>> {
        Ok(typst_html::html(doc))
    }
}

impl WorldComputable<LspCompilerFeat> for HtmlExport {
    fn compute(graph: &Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<Self> {
        Ok(Self(OptionDocument::run_export::<Self>(graph)?))
    }
}

pub struct TypliteMarkdownExport(pub Option<SourceResult<String>>);

impl TypliteMarkdownExport {
    fn needs_run(
        graph: &Arc<WorldComputeGraph<LspCompilerFeat>>,
        doc: Option<&TypstPagedDocument>,
        config: &ExportMarkdownTask,
    ) -> bool {
        let timing = config.export.when;
        ProjectCompilation::needs_run(&graph.snap, Some(timing), doc).unwrap_or_default()
    }

    fn run(
        graph: &Arc<WorldComputeGraph<LspCompilerFeat>>,
    ) -> Result<Option<SourceResult<String>>> {
        if !OptionDocument::needs_run(graph, Self::needs_run)? {
            return Ok(None);
        }

        let conv = Typlite::new(Arc::new(graph.snap.world.clone()))
            .convert()
            .map_err(|e| anyhow::anyhow!("failed to convert to markdown: {e}"))?;

        Ok(Some(Ok(conv.to_string())))
    }
}

impl WorldComputable<LspCompilerFeat> for TypliteMarkdownExport {
    fn compute(graph: &Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<Self> {
        Self::run(graph).map(Self)
    }
}

pub struct TextExport(pub Option<SourceResult<String>>);

impl ExportComputation<TypstPagedDocument> for TextExport {
    type Output = SourceResult<String>;
    type Config = ExportTextTask;

    fn needs_run(
        graph: &Arc<WorldComputeGraph<LspCompilerFeat>>,
        doc: Option<&TypstPagedDocument>,
        config: &Self::Config,
    ) -> bool {
        let timing = config.export.when;
        ProjectCompilation::needs_run(&graph.snap, Some(timing), doc).unwrap_or_default()
    }

    fn run(
        doc: &Arc<TypstPagedDocument>,
        _config: &ExportTextTask,
    ) -> Result<SourceResult<String>> {
        Ok(Ok(format!(
            "{}",
            FullTextDigest(TypstDocument::Paged(doc.clone()))
        )))
    }
}

impl WorldComputable<LspCompilerFeat> for TextExport {
    fn compute(graph: &Arc<WorldComputeGraph<LspCompilerFeat>>) -> Result<Self> {
        Ok(Self(OptionDocument::run_export::<Self>(graph)?))
    }
}

fn parse_color(fill: String) -> anyhow::Result<Color> {
    match fill.as_str() {
        "black" => Ok(Color::BLACK),
        "white" => Ok(Color::WHITE),
        "red" => Ok(Color::RED),
        "green" => Ok(Color::GREEN),
        "blue" => Ok(Color::BLUE),
        hex if hex.starts_with('#') => {
            Color::from_str(&hex[1..]).map_err(|e| anyhow::anyhow!("failed to parse color: {e}"))
        }
        _ => anyhow::bail!("invalid color: {fill}"),
    }
}

/// Convert [`chrono::DateTime`] to [`Timestamp`]
fn convert_datetime(date_time: chrono::DateTime<chrono::Utc>) -> Option<Timestamp> {
    use chrono::{Datelike, Timelike};
    let datetime = TypstDatetime::from_ymd_hms(
        date_time.year(),
        date_time.month().try_into().ok()?,
        date_time.day().try_into().ok()?,
        date_time.hour().try_into().ok()?,
        date_time.minute().try_into().ok()?,
        date_time.second().try_into().ok()?,
    );

    Some(Timestamp::new_utc(datetime.unwrap()))
}
