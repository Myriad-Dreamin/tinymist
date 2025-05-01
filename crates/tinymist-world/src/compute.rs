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
use crate::{CompilerFeat, CompilerWorld, EntryReader, TaskInputs};

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

    /// Creates a graph from the world.
    pub fn from_world(world: CompilerWorld<F>) -> Arc<Self> {
        Self::new(CompileSnapshot::from_world(world))
    }

    /// Clones the graph with the same snapshot.
    pub fn snapshot(&self) -> Arc<Self> {
        self.snapshot_unsafe(self.snap.clone())
    }

    /// Clones the graph with the same snapshot. Take care of the consistency by
    /// your self.
    pub fn snapshot_unsafe(&self, snap: CompileSnapshot<F>) -> Arc<Self> {
        Arc::new(Self {
            snap,
            entries: Mutex::new(self.entries.lock().clone()),
        })
    }

    /// Forks a new snapshot that compiles a different document.
    // todo: share cache if task doesn't change.
    pub fn task(&self, inputs: TaskInputs) -> Arc<Self> {
        let mut snap = self.snap.clone();
        snap = snap.task(inputs);
        Self::new(snap)
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

    pub fn world(&self) -> &CompilerWorld<F> {
        &self.snap.world
    }

    pub fn registry(&self) -> &Arc<F::Registry> {
        &self.snap.world.registry
    }

    pub fn library(&self) -> &typst::Library {
        &self.snap.world.library
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

impl<D: typst::Document + Send + Sync + 'static> CompilationTask<D> {
    pub fn ensure_main<F: CompilerFeat>(world: &CompilerWorld<F>) -> SourceResult<()> {
        let main_id = world.main_id();
        let checked = main_id.ok_or_else(|| typst::diag::eco_format!("entry file is not set"));
        checked.at(Span::detached()).map(|_| ())
    }

    pub fn execute<F: CompilerFeat>(world: &CompilerWorld<F>) -> Warned<SourceResult<Arc<D>>> {
        let res = Self::ensure_main(world);
        if let Err(err) = res {
            return Warned {
                output: Err(err),
                warnings: EcoVec::new(),
            };
        }

        let is_paged_compilation = TypeId::of::<D>() == TypeId::of::<TypstPagedDocument>();
        let is_html_compilation = TypeId::of::<D>() == TypeId::of::<TypstHtmlDocument>();

        let mut world = if is_paged_compilation {
            world.paged_task()
        } else if is_html_compilation {
            // todo: create html world once
            world.html_task()
        } else {
            Cow::Borrowed(world)
        };

        world.to_mut().set_is_compiling(true);
        let compiled = ::typst::compile::<D>(world.as_ref());
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
    }
}

impl<F: CompilerFeat, D> WorldComputable<F> for CompilationTask<D>
where
    D: typst::Document + Send + Sync + 'static,
{
    type Output = Option<Warned<SourceResult<Arc<D>>>>;

    fn compute(graph: &Arc<WorldComputeGraph<F>>) -> Result<Self::Output> {
        let enabled = graph.must_get::<FlagTask<CompilationTask<D>>>()?.enabled;

        Ok(enabled.then(|| CompilationTask::<D>::execute(&graph.snap.world)))
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

impl<F: CompilerFeat> WorldComputeGraph<F> {
    pub fn ensure_main(&self) -> SourceResult<()> {
        CompilationTask::<TypstPagedDocument>::ensure_main(&self.snap.world)
    }

    /// Compile once from scratch.
    pub fn pure_compile<D: ::typst::Document + Send + Sync + 'static>(
        &self,
    ) -> Warned<SourceResult<Arc<D>>> {
        CompilationTask::<D>::execute(&self.snap.world)
    }

    /// Compile once from scratch.
    pub fn compile(&self) -> Warned<SourceResult<Arc<TypstPagedDocument>>> {
        self.pure_compile()
    }

    /// Compile to html once from scratch.
    pub fn compile_html(&self) -> Warned<SourceResult<Arc<TypstHtmlDocument>>> {
        self.pure_compile()
    }

    /// Compile paged document with cache
    pub fn shared_compile(self: &Arc<Self>) -> Result<Option<Arc<TypstPagedDocument>>> {
        let doc = self.compute::<OptionDocumentTask<TypstPagedDocument>>()?;
        Ok(doc.as_ref().clone())
    }

    /// Compile HTML document with cache
    pub fn shared_compile_html(self: &Arc<Self>) -> Result<Option<Arc<TypstHtmlDocument>>> {
        let doc = self.compute::<OptionDocumentTask<TypstHtmlDocument>>()?;
        Ok(doc.as_ref().clone())
    }

    /// Gets the diagnostics from shared compilation.
    pub fn shared_diagnostics(self: &Arc<Self>) -> Result<Arc<DiagnosticsTask>> {
        self.compute::<DiagnosticsTask>()
    }
}
