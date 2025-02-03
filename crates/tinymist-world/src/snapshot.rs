//! Project compiler for tinymist.

use core::fmt;
use std::any::TypeId;
use std::sync::{Arc, OnceLock};

use crate::{CompilerFeat, CompilerWorld, EntryReader, TaskInputs};
use ecow::EcoString;
use parking_lot::Mutex;
use tinymist_std::error::prelude::*;
use tinymist_std::typst::TypstDocument;

/// Project instance id. This is slightly different from the project ids that
/// persist in disk.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectInsId(pub EcoString);

impl fmt::Display for ProjectInsId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl ProjectInsId {
    /// The primary project id.
    pub const PRIMARY: ProjectInsId = ProjectInsId(EcoString::inline("primary"));
}

/// A signal that possibly triggers an export.
///
/// Whether to export depends on the current state of the document and the user
/// settings.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExportSignal {
    /// Whether the revision is annotated by memory events.
    pub by_mem_events: bool,
    /// Whether the revision is annotated by file system events.
    pub by_fs_events: bool,
    /// Whether the revision is annotated by entry update.
    pub by_entry_update: bool,
}

/// A snapshot of the project and compilation state.
pub struct CompileSnapshot<F: CompilerFeat> {
    /// The project id.
    pub id: ProjectInsId,
    /// The export signal for the document.
    pub signal: ExportSignal,
    /// Using world
    pub world: CompilerWorld<F>,
    /// The last successfully compiled document.
    pub success_doc: Option<TypstDocument>,
}

impl<F: CompilerFeat + 'static> CompileSnapshot<F> {
    /// Creates a snapshot from the world.
    pub fn from_world(world: CompilerWorld<F>) -> Self {
        Self {
            id: ProjectInsId("primary".into()),
            signal: ExportSignal::default(),
            world,
            success_doc: None,
        }
    }

    /// Forks a new snapshot that compiles a different document.
    ///
    /// Note: the resulting document should not be shared in system, because we
    /// generally believe that the document is revisioned, but temporary
    /// tasks break this assumption.
    pub fn task(mut self, inputs: TaskInputs) -> Self {
        'check_changed: {
            if let Some(entry) = &inputs.entry {
                if *entry != self.world.entry_state() {
                    break 'check_changed;
                }
            }
            if let Some(inputs) = &inputs.inputs {
                if inputs.clone() != self.world.inputs() {
                    break 'check_changed;
                }
            }

            return self;
        };

        self.world = self.world.task(inputs);

        self
    }
}

impl<F: CompilerFeat> Clone for CompileSnapshot<F> {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            signal: self.signal,
            world: self.world.clone(),
            success_doc: self.success_doc.clone(),
        }
    }
}

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
