#![allow(missing_docs)]

use std::any::TypeId;
use std::borrow::Cow;
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use tinymist_std::error::prelude::*;
use tinymist_std::typst::{TypstHtmlDocument, TypstPagedDocument};
use typst::diag::{At, SourceResult, Warned};
use typst::ecow::EcoVec;
use typst::syntax::Span;

use crate::snapshot::CompileSnapshot;
use crate::{CompilerFeat, EntryReader};

type AnyArc = Arc<dyn std::any::Any + Send + Sync>;

/// A world compute entry.
#[derive(Debug, Clone, Default)]
struct WorldComputeEntry {
    computed: Arc<OnceLock<Result<AnyArc>>>,
}

impl WorldComputeEntry {
    fn cast<T: std::any::Any + Send + Sync>(e: Result<AnyArc>) -> Result<Arc<T>> {
        e.map(|e| e.downcast().expect("T is T"))
    }
}

/// A world compute graph.
pub struct WorldComputeGraph<F: CompilerFeat> {
    /// The used snapshot.
    pub snap: CompileSnapshot<F>,
    /// The computed entries.
    entries: Mutex<rpds::RedBlackTreeMapSync<TypeId, WorldComputeEntry>>,
}

/// A world computable trait.
pub trait WorldComputable<F: CompilerFeat>: std::any::Any + Send + Sync + Sized {
    type Output: Send + Sync + 'static;

    /// The computation implementation.
    ///
    /// ## Example
    ///
    /// The example shows that a computation can depend on specific world
    /// implementation. It computes the system font that only works on the
    /// system world.
    ///
    /// ```rust
    /// use std::sync::Arc;
    ///
    /// use tinymist_std::error::prelude::*;
    /// use tinymist_world::{WorldComputeGraph, WorldComputable};
    /// use tinymist_world::font::FontResolverImpl;
    /// use tinymist_world::system::SystemCompilerFeat;
    ///
    ///
    /// pub struct SystemFontsOnce {
    ///     fonts: Arc<FontResolverImpl>,
    /// }
    ///
    /// impl WorldComputable<SystemCompilerFeat> for SystemFontsOnce {
    ///     type Output = Self;
    ///
    ///     fn compute(graph: &Arc<WorldComputeGraph<SystemCompilerFeat>>) -> Result<Self> {
    ///
    ///         Ok(Self {
    ///             fonts: graph.snap.world.font_resolver.clone(),
    ///         })
    ///     }
    /// }
    ///
    /// /// Computes the system fonts.
    /// fn compute_system_fonts(graph: &Arc<WorldComputeGraph<SystemCompilerFeat>>) {
    ///    let _fonts = graph.compute::<SystemFontsOnce>().expect("font").fonts.clone();
    /// }
    /// ```
    fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self::Output>;
}

impl<F: CompilerFeat> WorldComputeGraph<F> {
    /// Creates a new world compute graph.
    pub fn new(snap: CompileSnapshot<F>) -> Arc<Self> {
        Arc::new(Self {
            snap,
            entries: Default::default(),
        })
    }

    /// Clones the graph with the same snapshot.
    pub fn snapshot(&self) -> Arc<Self> {
        Arc::new(Self {
            snap: self.snap.clone(),
            entries: Mutex::new(self.entries.lock().clone()),
        })
    }

    /// Gets a world computed.
    pub fn must_get<T: WorldComputable<F>>(&self) -> Result<Arc<T::Output>> {
        let res = self.get::<T>().transpose()?;
        res.with_context("computation not found", || {
            Some(Box::new([("type", std::any::type_name::<T>().to_owned())]))
        })
    }

    /// Gets a world computed.
    pub fn get<T: WorldComputable<F>>(&self) -> Option<Result<Arc<T::Output>>> {
        let computed = self.computed(TypeId::of::<T>()).computed;
        computed.get().cloned().map(WorldComputeEntry::cast)
    }

    pub fn exact_provide<T: WorldComputable<F>>(&self, ins: Result<Arc<T::Output>>) {
        if self.provide::<T>(ins).is_err() {
            panic!(
                "failed to provide computed instance: {:?}",
                std::any::type_name::<T>()
            );
        }
    }

    /// Provides some precomputed instance.
    #[must_use = "the result must be checked"]
    pub fn provide<T: WorldComputable<F>>(
        &self,
        ins: Result<Arc<T::Output>>,
    ) -> Result<(), Result<Arc<T::Output>>> {
        let entry = self.computed(TypeId::of::<T>()).computed;
        let initialized = entry.set(ins.map(|e| e as AnyArc));
        initialized.map_err(WorldComputeEntry::cast)
    }

    /// Gets or computes a world computable.
    pub fn compute<T: WorldComputable<F>>(self: &Arc<Self>) -> Result<Arc<T::Output>> {
        let entry = self.computed(TypeId::of::<T>()).computed;
        let computed = entry.get_or_init(|| Ok(Arc::new(T::compute(self)?)));
        WorldComputeEntry::cast(computed.clone())
    }

    fn computed(&self, id: TypeId) -> WorldComputeEntry {
        let mut entries = self.entries.lock();
        if let Some(entry) = entries.get(&id) {
            entry.clone()
        } else {
            let entry = WorldComputeEntry::default();
            entries.insert_mut(id, entry.clone());
            entry
        }
    }
}

pub trait ExportDetection<F: CompilerFeat, D> {
    type Config: Send + Sync + 'static;

    fn needs_run(graph: &Arc<WorldComputeGraph<F>>, config: &Self::Config) -> bool;
}

pub trait ExportComputation<F: CompilerFeat, D> {
    type Output;
    type Config: Send + Sync + 'static;

    fn run_with<C: WorldComputable<F, Output = Option<Arc<D>>>>(
        g: &Arc<WorldComputeGraph<F>>,
        config: &Self::Config,
    ) -> Result<Self::Output> {
        let doc = g.compute::<C>()?;
        let doc = doc.as_ref().as_ref().context("document not found")?;
        Self::run(g, doc, config)
    }

    fn cast_run<'a>(
        g: &Arc<WorldComputeGraph<F>>,
        doc: impl TryInto<&'a Arc<D>, Error = tinymist_std::Error>,
        config: &Self::Config,
    ) -> Result<Self::Output>
    where
        D: 'a,
    {
        Self::run(g, doc.try_into()?, config)
    }

    fn run(
        g: &Arc<WorldComputeGraph<F>>,
        doc: &Arc<D>,
        config: &Self::Config,
    ) -> Result<Self::Output>;
}

pub struct ConfigTask<T>(pub T);

impl<F: CompilerFeat, T: Send + Sync + 'static> WorldComputable<F> for ConfigTask<T> {
    type Output = T;

    fn compute(_graph: &Arc<WorldComputeGraph<F>>) -> Result<T> {
        let id = std::any::type_name::<T>();
        panic!("{id:?} must be provided before computation");
    }
}

pub type FlagTask<T> = ConfigTask<TaskFlagBase<T>>;
pub struct TaskFlagBase<T> {
    pub enabled: bool,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> FlagTask<T> {
    pub fn flag(flag: bool) -> Arc<TaskFlagBase<T>> {
        Arc::new(TaskFlagBase {
            enabled: flag,
            _phantom: Default::default(),
        })
    }
}

pub type PagedCompilationTask = CompilationTask<TypstPagedDocument>;
pub type HtmlCompilationTask = CompilationTask<TypstHtmlDocument>;

pub struct CompilationTask<D>(std::marker::PhantomData<D>);

impl<F: CompilerFeat, D> WorldComputable<F> for CompilationTask<D>
where
    D: typst::Document + Send + Sync + 'static,
{
    type Output = Option<Warned<SourceResult<Arc<D>>>>;

    fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self::Output> {
        let enabled = graph.must_get::<FlagTask<CompilationTask<D>>>()?.enabled;

        Ok(enabled.then(|| {
            // todo: create html world once
            let is_paged_compilation = TypeId::of::<D>() == TypeId::of::<TypstPagedDocument>();
            let is_html_compilation = TypeId::of::<D>() == TypeId::of::<TypstHtmlDocument>();

            let mut world = if is_paged_compilation {
                graph.snap.world.paged_task()
            } else if is_html_compilation {
                graph.snap.world.html_task()
            } else {
                Cow::Borrowed(&graph.snap.world)
            };

            world.to_mut().set_is_compiling(true);
            let compiled = typst::compile::<D>(world.as_ref());
            world.to_mut().set_is_compiling(false);

            let exclude_html_warnings = if !is_html_compilation {
                compiled.warnings
            } else if compiled.warnings.len() == 1
                && compiled.warnings[0]
                    .message
                    .starts_with("html export is under active development")
            {
                EcoVec::new()
            } else {
                compiled.warnings
            };

            Warned {
                output: compiled.output.map(Arc::new),
                warnings: exclude_html_warnings,
            }
        }))
    }
}

pub struct OptionDocumentTask<D>(std::marker::PhantomData<D>);

impl<F: CompilerFeat, D> WorldComputable<F> for OptionDocumentTask<D>
where
    D: typst::Document + Send + Sync + 'static,
{
    type Output = Option<Arc<D>>;

    fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self::Output> {
        let doc = graph.compute::<CompilationTask<D>>()?;
        let compiled = doc
            .as_ref()
            .as_ref()
            .and_then(|warned| warned.output.clone().ok());

        Ok(compiled)
    }
}

impl<D> OptionDocumentTask<D> where D: typst::Document + Send + Sync + 'static {}

struct CompilationDiagnostics {
    errors: Option<EcoVec<typst::diag::SourceDiagnostic>>,
    warnings: Option<EcoVec<typst::diag::SourceDiagnostic>>,
}

impl CompilationDiagnostics {
    fn from_result<T>(result: &Option<Warned<SourceResult<T>>>) -> Self {
        let errors = result
            .as_ref()
            .and_then(|r| r.output.as_ref().map_err(|e| e.clone()).err());
        let warnings = result.as_ref().map(|r| r.warnings.clone());

        Self { errors, warnings }
    }
}

pub struct DiagnosticsTask {
    paged: CompilationDiagnostics,
    html: CompilationDiagnostics,
}

impl<F: CompilerFeat> WorldComputable<F> for DiagnosticsTask {
    type Output = Self;

    fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self> {
        let paged = graph.compute::<PagedCompilationTask>()?.clone();
        let html = graph.compute::<HtmlCompilationTask>()?.clone();

        Ok(Self {
            paged: CompilationDiagnostics::from_result(&paged),
            html: CompilationDiagnostics::from_result(&html),
        })
    }
}

impl DiagnosticsTask {
    pub fn error_cnt(&self) -> usize {
        self.paged.errors.as_ref().map_or(0, |e| e.len())
            + self.html.errors.as_ref().map_or(0, |e| e.len())
    }

    pub fn warning_cnt(&self) -> usize {
        self.paged.warnings.as_ref().map_or(0, |e| e.len())
            + self.html.warnings.as_ref().map_or(0, |e| e.len())
    }

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

// pub type ErasedVecExportTask<E> = ErasedExportTask<SourceResult<Bytes>, E>;
// pub type ErasedStrExportTask<E> = ErasedExportTask<SourceResult<String>, E>;

// pub struct ErasedExportTask<T, E> {
//     _phantom: std::marker::PhantomData<(T, E)>,
// }

// #[allow(clippy::type_complexity)]
// struct ErasedExportImpl<F: CompilerFeat, T, E> {
//     f: Arc<dyn Fn(&Arc<WorldComputeGraph<F>>) -> Result<Option<T>> + Send +
// Sync>,     _phantom: std::marker::PhantomData<E>,
// }

// impl<T: Send + Sync + 'static, E: Send + Sync + 'static> ErasedExportTask<T,
// E> {     #[must_use = "the result must be checked"]
//     pub fn provide_raw<F: CompilerFeat>(
//         graph: &Arc<WorldComputeGraph<F>>,
//         f: impl Fn(&Arc<WorldComputeGraph<F>>) -> Result<Option<T>> + Send +
// Sync + 'static,     ) -> Result<()> {
//         let provided = graph.provide::<ConfigTask<ErasedExportImpl<F, T,
// E>>>(Ok(Arc::new({             ErasedExportImpl {
//                 f: Arc::new(f),
//                 _phantom: std::marker::PhantomData,
//             }
//         })));

//         if provided.is_err() {
//             tinymist_std::bail!("already provided")
//         }

//         Ok(())
//     }

//     #[must_use = "the result must be checked"]
//     pub fn provide<F: CompilerFeat, D, C>(graph: &Arc<WorldComputeGraph<F>>)
// -> Result<()>     where
//         D: typst::Document + Send + Sync + 'static,
//         C: WorldComputable<F> + ExportComputation<F, D, Output = T>,
//     {
//         Self::provide_raw(graph, OptionDocumentTask::run_export::<F, C>)
//     }
// }

// impl<F: CompilerFeat, T: Send + Sync + 'static, E: Send + Sync + 'static>
// WorldComputable<F>     for ErasedExportTask<T, E>
// {
//     type Output = Option<T>;

//     fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self::Output> {
//         let conf = graph.must_get::<ConfigTask<ErasedExportImpl<F, T,
// E>>>()?;         (conf.f)(graph)
//     }
// }

impl<F: CompilerFeat> WorldComputeGraph<F> {
    pub fn ensure_main(&self) -> SourceResult<()> {
        let main_id = self.snap.world.main_id();
        let checked = main_id.ok_or_else(|| typst::diag::eco_format!("entry file is not set"));
        checked.at(Span::detached()).map(|_| ())
    }

    /// Compile once from scratch.
    pub fn pure_compile<D: ::typst::Document>(&self) -> Warned<SourceResult<Arc<D>>> {
        let res = self.ensure_main();
        if let Err(err) = res {
            return Warned {
                output: Err(err),
                warnings: EcoVec::new(),
            };
        }

        let res = ::typst::compile::<D>(&self.snap.world);
        // compile document
        Warned {
            output: res.output.map(Arc::new),
            warnings: res.warnings,
        }
    }

    /// Compile once from scratch.
    pub fn compile(&self) -> Warned<SourceResult<Arc<TypstPagedDocument>>> {
        self.pure_compile()
    }

    /// Compile to html once from scratch.
    pub fn compile_html(&self) -> Warned<SourceResult<Arc<::typst::html::HtmlDocument>>> {
        self.pure_compile()
    }

    // With **the compilation state**, query the matches for the selector.
    // fn query(&mut self, selector: String, document: &TypstDocument) ->
    // SourceResult<Vec<Content>> {     self.pure_query(world, selector,
    // document) }
}
