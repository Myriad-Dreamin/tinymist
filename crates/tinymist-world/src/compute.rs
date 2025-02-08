#![allow(missing_docs)]

use std::any::TypeId;
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use tinymist_std::error::prelude::*;
use tinymist_std::typst::{TypstHtmlDocument, TypstPagedDocument};
use typst::diag::{SourceResult, Warned};
use typst::ecow::EcoVec;

use crate::snapshot::CompileSnapshot;
use crate::CompilerFeat;

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
    /// The computation implementation.
    ///
    /// ## Example
    ///
    /// The example shows that a computation can depend on specific world
    /// implementation. It computes the system font that only works on the
    /// system world.
    ///
    /// ```rust
    /// pub struct SystemFontsOnce {
    ///     fonts: Arc<FontResolverImpl>,
    /// }
    ///
    /// impl WorldComputable<SystemCompilerFeat> for SystemFontsOnce {
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
    ///    let _fonts = graph.compute::<FontsOnce>().expect("font").fonts.clone();
    /// }
    /// ```
    fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self>;
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
    pub fn must_get<T: WorldComputable<F>>(&self) -> Result<Arc<T>> {
        let res = self.get::<T>().transpose()?;
        res.with_context("computation not found", || {
            Some(Box::new([("type", std::any::type_name::<T>().to_owned())]))
        })
    }

    /// Gets a world computed.
    pub fn get<T: WorldComputable<F>>(&self) -> Option<Result<Arc<T>>> {
        let computed = self.computed(TypeId::of::<T>()).computed;
        computed.get().cloned().map(WorldComputeEntry::cast)
    }

    pub fn exact_provide<T: WorldComputable<F>>(&self, ins: Result<Arc<T>>) {
        if self.provide(ins).is_err() {
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
        ins: Result<Arc<T>>,
    ) -> Result<(), Result<Arc<T>>> {
        let entry = self.computed(TypeId::of::<T>()).computed;
        let initialized = entry.set(ins.map(|e| Arc::new(e) as AnyArc));
        initialized.map_err(WorldComputeEntry::cast)
    }

    /// Gets or computes a world computable.
    pub fn compute<T: WorldComputable<F>>(self: &Arc<Self>) -> Result<Arc<T>> {
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

pub trait ExportComputation<F: CompilerFeat, D> {
    type Output;
    type Config: Send + Sync + 'static;

    fn needs_run(graph: &Arc<WorldComputeGraph<F>>, doc: Option<&D>, config: &Self::Config)
        -> bool;

    fn run(doc: &Arc<D>, config: &Self::Config) -> Result<Self::Output>;
}

pub struct ConfigTask<T>(pub T);

impl<F: CompilerFeat, T: Send + Sync + 'static> WorldComputable<F> for ConfigTask<T> {
    fn compute(_graph: &Arc<WorldComputeGraph<F>>) -> Result<Self> {
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
    pub fn flag(flag: bool) -> Arc<Self> {
        Arc::new(ConfigTask(TaskFlagBase {
            enabled: flag,
            _phantom: Default::default(),
        }))
    }
}

pub type PagedCompilationTask = CompilationTask<TypstPagedDocument>;
pub type HtmlCompilationTask = CompilationTask<TypstHtmlDocument>;

pub struct CompilationTask<D>(Option<Warned<SourceResult<Arc<D>>>>);

impl<F: CompilerFeat, D> WorldComputable<F> for CompilationTask<D>
where
    D: typst::Document + Send + Sync + 'static,
{
    fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self> {
        let enabled = graph.must_get::<FlagTask<CompilationTask<D>>>()?.0.enabled;

        Ok(Self(enabled.then(|| {
            let compiled = typst::compile::<D>(&graph.snap.world);
            Warned {
                output: compiled.output.map(Arc::new),
                warnings: compiled.warnings,
            }
        })))
    }
}

pub struct OptionDocumentTask<D>(pub Option<Arc<D>>);

impl<F: CompilerFeat, D> WorldComputable<F> for OptionDocumentTask<D>
where
    D: typst::Document + Send + Sync + 'static,
{
    fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self> {
        let doc = graph.compute::<CompilationTask<D>>()?;
        let compiled = doc.0.as_ref().and_then(|warned| warned.output.clone().ok());

        Ok(Self(compiled))
    }
}

impl<D> OptionDocumentTask<D>
where
    D: typst::Document + Send + Sync + 'static,
{
    pub fn needs_run<F: CompilerFeat, C: Send + Sync + 'static>(
        graph: &Arc<WorldComputeGraph<F>>,
        f: impl FnOnce(&Arc<WorldComputeGraph<F>>, Option<&D>, &C) -> bool,
    ) -> Result<bool> {
        let Some(config) = graph.get::<ConfigTask<C>>().transpose()? else {
            return Ok(false);
        };

        let doc = graph.compute::<OptionDocumentTask<D>>()?;
        Ok(f(graph, doc.0.as_deref(), &config.0))
    }

    pub fn run_export<F: CompilerFeat, T: ExportComputation<F, D>>(
        graph: &Arc<WorldComputeGraph<F>>,
    ) -> Result<Option<T::Output>> {
        if !OptionDocumentTask::needs_run(graph, T::needs_run)? {
            return Ok(None);
        }

        let doc = graph.compute::<OptionDocumentTask<D>>()?.0.clone();
        let config = graph.get::<ConfigTask<T::Config>>().transpose()?;

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

pub struct DiagnosticsTask {
    paged: CompilationDiagnostics,
    html: CompilationDiagnostics,
}

impl<F: CompilerFeat> WorldComputable<F> for DiagnosticsTask {
    fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self> {
        let paged = graph.compute::<PagedCompilationTask>()?.0.clone();
        let html = graph.compute::<HtmlCompilationTask>()?.0.clone();

        Ok(Self {
            paged: CompilationDiagnostics::from_result(paged),
            html: CompilationDiagnostics::from_result(html),
        })
    }
}

impl DiagnosticsTask {
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

pub type ErasedVecExportTask<E> = ErasedExportTask<SourceResult<Vec<u8>>, E>;
pub type ErasedStrExportTask<E> = ErasedExportTask<SourceResult<String>, E>;

pub struct ErasedExportTask<T, E> {
    pub result: Option<T>,
    _phantom: std::marker::PhantomData<E>,
}

#[allow(clippy::type_complexity)]
struct ErasedExportImpl<F: CompilerFeat, T, E> {
    f: Arc<dyn Fn(&Arc<WorldComputeGraph<F>>) -> Result<ErasedExportTask<T, E>> + Send + Sync>,
}

impl<T: Send + Sync + 'static, E: Send + Sync + 'static> ErasedExportTask<T, E> {
    #[must_use = "the result must be checked"]
    pub fn provide_raw<F: CompilerFeat>(
        graph: &Arc<WorldComputeGraph<F>>,
        f: impl Fn(&Arc<WorldComputeGraph<F>>) -> Result<Option<T>> + Send + Sync + 'static,
    ) -> Result<()> {
        let provided = graph.provide::<ConfigTask<ErasedExportImpl<F, T, E>>>(Ok(Arc::new({
            ConfigTask(ErasedExportImpl {
                f: Arc::new(move |graph| {
                    let result = f(graph)?;
                    Ok(ErasedExportTask {
                        result,
                        _phantom: std::marker::PhantomData,
                    })
                }),
            })
        })));

        if provided.is_err() {
            tinymist_std::bail!("already provided")
        }

        Ok(())
    }

    #[must_use = "the result must be checked"]
    pub fn provide<F: CompilerFeat, D, C>(graph: &Arc<WorldComputeGraph<F>>) -> Result<()>
    where
        D: typst::Document + Send + Sync + 'static,
        C: WorldComputable<F> + ExportComputation<F, D, Output = T>,
    {
        Self::provide_raw(graph, OptionDocumentTask::run_export::<F, C>)
    }
}

impl<F: CompilerFeat, T: Send + Sync + 'static, E: Send + Sync + 'static> WorldComputable<F>
    for ErasedExportTask<T, E>
{
    fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self> {
        let f = graph.must_get::<ConfigTask<ErasedExportImpl<F, T, E>>>()?;
        (f.0.f)(graph)
    }
}
