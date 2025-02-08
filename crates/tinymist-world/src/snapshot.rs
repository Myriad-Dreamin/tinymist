//! Project compiler for tinymist.

use core::fmt;

use crate::{CompilerFeat, CompilerWorld, EntryReader, TaskInputs};
use ecow::EcoString;
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
