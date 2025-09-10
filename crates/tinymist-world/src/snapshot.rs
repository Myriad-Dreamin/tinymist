//! Project compiler for tinymist.

use core::fmt;

use crate::{CompilerFeat, CompilerWorld, EntryReader, TaskInputs, args::TaskWhen};
use ecow::EcoString;
use serde::{Deserialize, Serialize};
use tinymist_std::typst::TypstDocument;

/// Project instance id. This is slightly different from the project ids that
/// persist in disk.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectInsId(pub EcoString);

impl fmt::Display for ProjectInsId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl ProjectInsId {
    /// The primary project id.
    pub const PRIMARY: ProjectInsId = ProjectInsId(EcoString::inline("primary"));
}

/// The export signal for the document.
#[deprecated(note = "Use `CompileSignal` directly.")]
pub type ExportSignal = CompileSignal;

/// A signal that possibly triggers a compile (export).
///
/// Whether to compile (export) depends on the current state of the document and
/// the user settings.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileSignal {
    /// Whether the revision is annotated by memory events.
    pub by_mem_events: bool,
    /// Whether the revision is annotated by file system events.
    pub by_fs_events: bool,
    /// Whether the revision is annotated by entry update.
    pub by_entry_update: bool,
}

impl CompileSignal {
    /// Merges two signals.
    pub fn merge(&mut self, other: CompileSignal) {
        self.by_mem_events |= other.by_mem_events;
        self.by_fs_events |= other.by_fs_events;
        self.by_entry_update |= other.by_entry_update;
    }

    /// Whether there is any reason to compile (export).
    ///
    /// This is used to determine if the document should be compiled.
    pub fn any(&self) -> bool {
        self.by_mem_events || self.by_fs_events || self.by_entry_update
    }

    /// Excludes some signals.
    pub fn exclude(&self, excluded: Self) -> Self {
        Self {
            by_mem_events: self.by_mem_events && !excluded.by_mem_events,
            by_fs_events: self.by_fs_events && !excluded.by_fs_events,
            by_entry_update: self.by_entry_update && !excluded.by_entry_update,
        }
    }

    /// Whether the task should run.
    pub fn should_run_task_dyn(
        &self,
        when: &TaskWhen,
        docs: Option<&TypstDocument>,
    ) -> Option<bool> {
        match docs {
            Some(TypstDocument::Paged(doc)) => self.should_run_task(when, Some(doc.as_ref())),
            Some(TypstDocument::Html(doc)) => self.should_run_task(when, Some(doc.as_ref())),
            None => self.should_run_task::<typst::layout::PagedDocument>(when, None),
        }
    }

    /// Whether the task should run.
    pub fn should_run_task<D: typst::Document>(
        &self,
        when: &TaskWhen,
        docs: Option<&D>,
    ) -> Option<bool> {
        match when {
            TaskWhen::Never => Some(false),
            // todo: by script
            TaskWhen::Script => Some(self.by_entry_update),
            TaskWhen::OnType => Some(self.by_mem_events),
            TaskWhen::OnSave => Some(self.by_fs_events),
            TaskWhen::OnDocumentHasTitle if self.by_fs_events => {
                docs.map(|doc| doc.info().title.is_some())
            }
            TaskWhen::OnDocumentHasTitle => Some(false),
        }
    }
}

/// A snapshot of the project and compilation state.
///
/// This is used to store the state of the project and compilation.
pub struct CompileSnapshot<F: CompilerFeat> {
    /// The project id.
    pub id: ProjectInsId,
    /// The export signal for the document.
    pub signal: CompileSignal,
    /// The world.
    pub world: CompilerWorld<F>,
    /// The last successfully compiled document.
    pub success_doc: Option<TypstDocument>,
}

impl<F: CompilerFeat + 'static> CompileSnapshot<F> {
    /// Creates a snapshot from the world.
    pub fn from_world(world: CompilerWorld<F>) -> Self {
        Self {
            id: ProjectInsId("primary".into()),
            signal: CompileSignal::default(),
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
            if let Some(entry) = &inputs.entry
                && *entry != self.world.entry_state()
            {
                break 'check_changed;
            }
            if let Some(inputs) = &inputs.inputs
                && inputs.clone() != self.world.inputs()
            {
                break 'check_changed;
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
