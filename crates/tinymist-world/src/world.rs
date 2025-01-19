use std::{
    num::NonZeroUsize,
    ops::Deref,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, OnceLock},
};

use chrono::{DateTime, Datelike, Local};
use tinymist_std::error::prelude::*;
use tinymist_vfs::{
    FsProvider, PathResolution, RevisingVfs, SourceCache, TypstFileId, Vfs, WorkspaceResolver,
};
use typst::{
    diag::{eco_format, At, EcoString, FileError, FileResult, SourceResult},
    foundations::{Bytes, Datetime, Dict},
    syntax::{FileId, Source, Span, VirtualPath},
    text::{Font, FontBook},
    utils::LazyHash,
    Library, World,
};

use crate::parser::{
    get_semantic_tokens_full, get_semantic_tokens_legend, OffsetEncoding, SemanticToken,
    SemanticTokensLegend,
};
use crate::{
    package::{PackageRegistry, PackageSpec},
    source::SourceDb,
};
// use crate::source::{SharedState, SourceCache, SourceDb};
use crate::entry::{EntryManager, EntryReader, EntryState, DETACHED_ENTRY};
use crate::{font::FontResolver, CompilerFeat, ShadowApi, WorldDeps};

type CodespanResult<T> = Result<T, CodespanError>;
type CodespanError = codespan_reporting::files::Error;

/// A universe that provides access to the operating system.
///
/// Use [`CompilerUniverse::new`] to create a new universe.
/// Use [`CompilerUniverse::snapshot`] to create a new world.
#[derive(Debug)]
pub struct CompilerUniverse<F: CompilerFeat> {
    /// State for the *root & entry* of compilation.
    /// The world forbids direct access to files outside this directory.
    entry: EntryState,
    /// Additional input arguments to compile the entry file.
    inputs: Arc<LazyHash<Dict>>,

    /// Provides font management for typst compiler.
    pub font_resolver: Arc<F::FontResolver>,
    /// Provides package management for typst compiler.
    pub registry: Arc<F::Registry>,
    /// Provides path-based data access for typst compiler.
    vfs: Vfs<F::AccessModel>,

    /// The current revision of the universe.
    pub revision: NonZeroUsize,
}

/// Creates, snapshots, and manages the compiler universe.
impl<F: CompilerFeat> CompilerUniverse<F> {
    /// Create a [`CompilerUniverse`] with feature implementation.
    ///
    /// Although this function is public, it is always unstable and not intended
    /// to be used directly.
    /// + See [`crate::TypstSystemUniverse::new`] for system environment.
    /// + See [`crate::TypstBrowserUniverse::new`] for browser environment.
    pub fn new_raw(
        entry: EntryState,
        inputs: Option<Arc<LazyHash<Dict>>>,
        vfs: Vfs<F::AccessModel>,
        registry: Arc<F::Registry>,
        font_resolver: Arc<F::FontResolver>,
    ) -> Self {
        Self {
            entry,
            inputs: inputs.unwrap_or_default(),

            revision: NonZeroUsize::new(1).expect("initial revision is 1"),

            font_resolver,
            registry,
            vfs,
        }
    }

    /// Wrap driver with a given entry file.
    pub fn with_entry_file(mut self, entry_file: PathBuf) -> Self {
        let _ = self.increment_revision(|this| this.set_entry_file_(entry_file.as_path().into()));
        self
    }

    pub fn inputs(&self) -> Arc<LazyHash<Dict>> {
        self.inputs.clone()
    }

    pub fn snapshot(&self) -> CompilerWorld<F> {
        self.snapshot_with(None)
    }

    pub fn snapshot_with(&self, mutant: Option<TaskInputs>) -> CompilerWorld<F> {
        let w = CompilerWorld {
            entry: self.entry.clone(),
            inputs: self.inputs.clone(),
            library: create_library(self.inputs.clone()),
            font_resolver: self.font_resolver.clone(),
            registry: self.registry.clone(),
            vfs: self.vfs.snapshot(),
            revision: self.revision,
            source_db: SourceDb {
                is_compiling: true,
                slots: Default::default(),
            },
            now: OnceLock::new(),
        };

        mutant.map(|m| w.task(m)).unwrap_or(w)
    }

    /// Increment revision with actions.
    pub fn increment_revision<T>(&mut self, f: impl FnOnce(&mut RevisingUniverse<F>) -> T) -> T {
        f(&mut RevisingUniverse {
            vfs_revision: self.vfs.revision(),
            font_revision: self.font_resolver.revision(),
            registry_revision: self.registry.revision(),
            view_changed: false,
            inner: self,
        })
    }

    /// Mutate the entry state and return the old state.
    fn mutate_entry_(&mut self, mut state: EntryState) -> SourceResult<EntryState> {
        std::mem::swap(&mut self.entry, &mut state);
        Ok(state)
    }

    /// set an entry file.
    fn set_entry_file_(&mut self, entry_file: Arc<Path>) -> SourceResult<()> {
        let state = self.entry_state();
        let state = state
            .try_select_path_in_workspace(&entry_file)
            .map_err(|e| eco_format!("cannot select entry file out of workspace: {e}"))
            .at(Span::detached())?
            .ok_or_else(|| eco_format!("failed to determine root"))
            .at(Span::detached())?;

        self.mutate_entry_(state).map(|_| ())?;
        Ok(())
    }
}

impl<F: CompilerFeat> CompilerUniverse<F> {
    /// Reset the world for a new lifecycle (of garbage collection).
    pub fn reset(&mut self) {
        self.vfs.reset_all();
        // todo: shared state
    }

    /// Clear the vfs cache that is not touched for a long time.
    pub fn evict(&mut self, vfs_threshold: usize) {
        self.vfs.reset_access_model();
        self.vfs.evict(vfs_threshold);
    }

    /// Resolve the real path for a file id.
    pub fn path_for_id(&self, id: FileId) -> Result<PathResolution, FileError> {
        self.vfs.file_path(id)
    }

    /// Resolve the root of the workspace.
    pub fn id_for_path(&self, path: &Path) -> Option<FileId> {
        let root = self.entry.workspace_root()?;
        Some(WorkspaceResolver::workspace_file(
            Some(&root),
            VirtualPath::new(path.strip_prefix(&root).ok()?),
        ))
    }

    pub fn get_semantic_token_legend(&self) -> Arc<SemanticTokensLegend> {
        Arc::new(get_semantic_tokens_legend())
    }

    pub fn get_semantic_tokens(
        &self,
        file_path: Option<String>,
        encoding: OffsetEncoding,
    ) -> ZResult<Arc<Vec<SemanticToken>>> {
        let world = match file_path {
            Some(e) => {
                let path = Path::new(&e);
                let s = self
                    .entry_state()
                    .try_select_path_in_workspace(path)?
                    .ok_or_else(|| error_once!("cannot select file", path: e))?;

                self.snapshot_with(Some(TaskInputs {
                    entry: Some(s),
                    inputs: None,
                }))
            }
            None => self.snapshot(),
        };

        let src = world
            .source(world.main())
            .map_err(|e| error_once!("cannot access source file", err: e))?;
        Ok(Arc::new(get_semantic_tokens_full(&src, encoding)))
    }
}

impl<F: CompilerFeat> ShadowApi for CompilerUniverse<F> {
    #[inline]
    fn reset_shadow(&mut self) {
        self.increment_revision(|this| this.vfs.revise().reset_shadow())
    }

    fn shadow_paths(&self) -> Vec<Arc<Path>> {
        self.vfs.shadow_paths()
    }

    fn shadow_ids(&self) -> Vec<TypstFileId> {
        self.vfs.shadow_ids()
    }

    #[inline]
    fn map_shadow(&mut self, path: &Path, content: Bytes) -> FileResult<()> {
        self.increment_revision(|this| this.vfs().map_shadow(path, Ok(content).into()))
    }

    #[inline]
    fn unmap_shadow(&mut self, path: &Path) -> FileResult<()> {
        self.increment_revision(|this| this.vfs().unmap_shadow(path))
    }

    #[inline]
    fn map_shadow_by_id(&mut self, file_id: FileId, content: Bytes) -> FileResult<()> {
        self.increment_revision(|this| this.vfs().map_shadow_by_id(file_id, Ok(content).into()))
    }

    #[inline]
    fn unmap_shadow_by_id(&mut self, file_id: FileId) -> FileResult<()> {
        self.increment_revision(|this| {
            this.vfs().remove_shadow_by_id(file_id);
            Ok(())
        })
    }
}

impl<F: CompilerFeat> EntryReader for CompilerUniverse<F> {
    fn entry_state(&self) -> EntryState {
        self.entry.clone()
    }
}

impl<F: CompilerFeat> EntryManager for CompilerUniverse<F> {
    fn mutate_entry(&mut self, state: EntryState) -> SourceResult<EntryState> {
        self.increment_revision(|this| this.mutate_entry_(state))
    }
}

pub struct RevisingUniverse<'a, F: CompilerFeat> {
    view_changed: bool,
    vfs_revision: NonZeroUsize,
    font_revision: Option<NonZeroUsize>,
    registry_revision: Option<NonZeroUsize>,
    pub inner: &'a mut CompilerUniverse<F>,
}

impl<F: CompilerFeat> std::ops::Deref for RevisingUniverse<'_, F> {
    type Target = CompilerUniverse<F>;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<F: CompilerFeat> std::ops::DerefMut for RevisingUniverse<'_, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

impl<F: CompilerFeat> Drop for RevisingUniverse<'_, F> {
    fn drop(&mut self) {
        let mut view_changed = self.view_changed;
        // If the revision is none, it means the fonts should be viewed as
        // changed unconditionally.
        if self.font_changed() {
            view_changed = true;
        }
        // If the revision is none, it means the packages should be viewed as
        // changed unconditionally.
        if self.registry_changed() {
            view_changed = true;

            // The registry has changed affects the vfs cache.
            self.vfs().reset_cache();
        }
        let view_changed = view_changed || self.vfs_changed();

        if view_changed {
            self.vfs.reset_access_model();
            let revision = &mut self.revision;
            *revision = revision.checked_add(1).unwrap();
        }
    }
}

impl<F: CompilerFeat> RevisingUniverse<'_, F> {
    pub fn vfs(&mut self) -> RevisingVfs<'_, F::AccessModel> {
        self.vfs.revise()
    }

    pub fn set_fonts(&mut self, fonts: Arc<F::FontResolver>) {
        self.inner.font_resolver = fonts;
    }

    pub fn set_package(&mut self, packages: Arc<F::Registry>) {
        self.inner.registry = packages;
    }

    /// Set the inputs for the compiler.
    pub fn set_inputs(&mut self, inputs: Arc<LazyHash<Dict>>) {
        self.view_changed = true;
        self.inner.inputs = inputs;
    }

    pub fn set_entry_file(&mut self, entry_file: Arc<Path>) -> SourceResult<()> {
        self.view_changed = true;
        self.inner.set_entry_file_(entry_file)
    }

    pub fn mutate_entry(&mut self, state: EntryState) -> SourceResult<EntryState> {
        self.view_changed = true;

        // Resets the cache if the workspace root has changed.
        let root_changed = self.inner.entry.workspace_root() == state.workspace_root();
        if root_changed {
            self.vfs().reset_cache();
        }

        self.inner.mutate_entry_(state)
    }

    pub fn flush(&mut self) {
        self.view_changed = true;
    }

    pub fn font_changed(&self) -> bool {
        is_revision_changed(self.font_revision, self.font_resolver.revision())
    }

    pub fn registry_changed(&self) -> bool {
        is_revision_changed(self.registry_revision, self.registry.revision())
    }

    pub fn vfs_changed(&self) -> bool {
        self.vfs_revision != self.vfs.revision()
    }
}

fn is_revision_changed(a: Option<NonZeroUsize>, b: Option<NonZeroUsize>) -> bool {
    a.is_none() || b.is_none() || a != b
}

pub struct CompilerWorld<F: CompilerFeat> {
    /// State for the *root & entry* of compilation.
    /// The world forbids direct access to files outside this directory.
    entry: EntryState,
    /// Additional input arguments to compile the entry file.
    inputs: Arc<LazyHash<Dict>>,

    /// Provides library for typst compiler.
    pub library: Arc<LazyHash<Library>>,
    /// Provides font management for typst compiler.
    pub font_resolver: Arc<F::FontResolver>,
    /// Provides package management for typst compiler.
    pub registry: Arc<F::Registry>,
    /// Provides path-based data access for typst compiler.
    vfs: Vfs<F::AccessModel>,

    revision: NonZeroUsize,
    /// Provides source database for typst compiler.
    source_db: SourceDb,
    /// The current datetime if requested. This is stored here to ensure it is
    /// always the same within one compilation. Reset between compilations.
    now: OnceLock<DateTime<Local>>,
}

impl<F: CompilerFeat> Clone for CompilerWorld<F> {
    fn clone(&self) -> Self {
        self.task(TaskInputs::default())
    }
}

#[derive(Default)]
pub struct TaskInputs {
    pub entry: Option<EntryState>,
    pub inputs: Option<Arc<LazyHash<Dict>>>,
}

impl<F: CompilerFeat> CompilerWorld<F> {
    pub fn task(&self, mutant: TaskInputs) -> CompilerWorld<F> {
        // Fetch to avoid inconsistent state.
        let _ = self.today(None);

        let library = mutant.inputs.clone().map(create_library);

        let root_changed = if let Some(e) = mutant.entry.as_ref() {
            self.entry.workspace_root() != e.workspace_root()
        } else {
            false
        };

        let mut world = CompilerWorld {
            inputs: mutant.inputs.unwrap_or_else(|| self.inputs.clone()),
            library: library.unwrap_or_else(|| self.library.clone()),
            entry: mutant.entry.unwrap_or_else(|| self.entry.clone()),
            font_resolver: self.font_resolver.clone(),
            registry: self.registry.clone(),
            vfs: self.vfs.snapshot(),
            revision: self.revision,
            source_db: self.source_db.clone(),
            now: self.now.clone(),
        };

        if root_changed {
            world.vfs.revise().reset_cache();
        }

        world
    }

    pub fn take_state(&mut self) -> SourceCache {
        self.vfs.take_state()
    }

    /// Sets flag to indicate whether the compiler is currently compiling.
    /// Note: Since `CompilerWorld` can be cloned, you can clone the world and
    /// set the flag then to avoid affecting the original world.
    pub fn set_is_compiling(&mut self, is_compiling: bool) {
        self.source_db.is_compiling = is_compiling;
    }

    pub fn inputs(&self) -> Arc<LazyHash<Dict>> {
        self.inputs.clone()
    }

    /// Resolve the real path for a file id.
    pub fn path_for_id(&self, id: FileId) -> Result<PathResolution, FileError> {
        self.vfs.file_path(id)
    }

    /// Resolve the root of the workspace.
    pub fn id_for_path(&self, path: &Path) -> Option<FileId> {
        let root = self.entry.workspace_root()?;
        Some(WorkspaceResolver::workspace_file(
            Some(&root),
            VirtualPath::new(path.strip_prefix(&root).ok()?),
        ))
    }

    /// Lookup a source file by id.
    #[track_caller]
    fn lookup(&self, id: FileId) -> Source {
        self.source(id)
            .expect("file id does not point to any source file")
    }

    fn map_source_or_default<T>(
        &self,
        id: FileId,
        default_v: T,
        f: impl FnOnce(Source) -> CodespanResult<T>,
    ) -> CodespanResult<T> {
        match World::source(self, id).ok() {
            Some(source) => f(source),
            None => Ok(default_v),
        }
    }

    pub fn revision(&self) -> NonZeroUsize {
        self.revision
    }
}

impl<F: CompilerFeat> ShadowApi for CompilerWorld<F> {
    #[inline]
    fn shadow_ids(&self) -> Vec<TypstFileId> {
        self.vfs.shadow_ids()
    }

    #[inline]
    fn shadow_paths(&self) -> Vec<Arc<Path>> {
        self.vfs.shadow_paths()
    }

    #[inline]
    fn reset_shadow(&mut self) {
        self.vfs.revise().reset_shadow()
    }

    #[inline]
    fn map_shadow(&mut self, path: &Path, content: Bytes) -> FileResult<()> {
        self.vfs.revise().map_shadow(path, Ok(content).into())
    }

    #[inline]
    fn unmap_shadow(&mut self, path: &Path) -> FileResult<()> {
        self.vfs.revise().unmap_shadow(path)
    }

    #[inline]
    fn map_shadow_by_id(&mut self, file_id: TypstFileId, content: Bytes) -> FileResult<()> {
        self.vfs
            .revise()
            .map_shadow_by_id(file_id, Ok(content).into())
    }

    #[inline]
    fn unmap_shadow_by_id(&mut self, file_id: TypstFileId) -> FileResult<()> {
        self.vfs.revise().remove_shadow_by_id(file_id);
        Ok(())
    }
}

impl<F: CompilerFeat> FsProvider for CompilerWorld<F> {
    fn file_path(&self, file_id: TypstFileId) -> FileResult<PathResolution> {
        self.vfs.file_path(file_id)
    }

    fn read(&self, file_id: TypstFileId) -> FileResult<Bytes> {
        self.vfs.read(file_id)
    }

    fn read_source(&self, file_id: TypstFileId) -> FileResult<Source> {
        self.vfs.source(file_id)
    }
}

impl<F: CompilerFeat> World for CompilerWorld<F> {
    /// The standard library.
    fn library(&self) -> &LazyHash<Library> {
        self.library.as_ref()
    }

    /// Access the main source file.
    fn main(&self) -> FileId {
        self.entry.main().unwrap_or_else(|| *DETACHED_ENTRY)
    }

    /// Metadata about all known fonts.
    fn font(&self, id: usize) -> Option<Font> {
        self.font_resolver.font(id)
    }

    /// Try to access the specified file.
    fn book(&self) -> &LazyHash<FontBook> {
        self.font_resolver.font_book()
    }

    /// Try to access the specified source file.
    ///
    /// The returned `Source` file's [id](Source::id) does not have to match the
    /// given `id`. Due to symlinks, two different file id's can point to the
    /// same on-disk file. Implementors can deduplicate and return the same
    /// `Source` if they want to, but do not have to.
    fn source(&self, id: FileId) -> FileResult<Source> {
        static DETACH_SOURCE: LazyLock<Source> =
            LazyLock::new(|| Source::new(*DETACHED_ENTRY, String::new()));

        if id == *DETACHED_ENTRY {
            return Ok(DETACH_SOURCE.clone());
        }

        self.source_db.source(id, self)
    }

    /// Try to access the specified file.
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.source_db.file(id, self)
    }

    /// Get the current date.
    ///
    /// If no offset is specified, the local date should be chosen. Otherwise,
    /// the UTC date should be chosen with the corresponding offset in hours.
    ///
    /// If this function returns `None`, Typst's `datetime` function will
    /// return an error.
    fn today(&self, offset: Option<i64>) -> Option<Datetime> {
        let now = self.now.get_or_init(|| tinymist_std::time::now().into());

        let naive = match offset {
            None => now.naive_local(),
            Some(o) => now.naive_utc() + chrono::Duration::try_hours(o)?,
        };

        Datetime::from_ymd(
            naive.year(),
            naive.month().try_into().ok()?,
            naive.day().try_into().ok()?,
        )
    }

    /// A list of all available packages and optionally descriptions for them.
    ///
    /// This function is optional to implement. It enhances the user experience
    /// by enabling autocompletion for packages. Details about packages from the
    /// `@preview` namespace are available from
    /// `https://packages.typst.org/preview/index.json`.
    fn packages(&self) -> &[(PackageSpec, Option<EcoString>)] {
        self.registry.packages()
    }
}

impl<F: CompilerFeat> EntryReader for CompilerWorld<F> {
    fn entry_state(&self) -> EntryState {
        self.entry.clone()
    }
}

impl<F: CompilerFeat> WorldDeps for CompilerWorld<F> {
    #[inline]
    fn iter_dependencies(&self, f: &mut dyn FnMut(TypstFileId)) {
        self.source_db.iter_dependencies_dyn(f)
    }
}

impl<'a, F: CompilerFeat> codespan_reporting::files::Files<'a> for CompilerWorld<F> {
    /// A unique identifier for files in the file provider. This will be used
    /// for rendering `diagnostic::Label`s in the corresponding source files.
    type FileId = FileId;

    /// The user-facing name of a file, to be displayed in diagnostics.
    type Name = String;

    /// The source code of a file.
    type Source = Source;

    /// The user-facing name of a file.
    fn name(&'a self, id: FileId) -> CodespanResult<Self::Name> {
        Ok(match self.path_for_id(id) {
            Ok(path) => path.as_path().display().to_string(),
            Err(_) => format!("{id:?}"),
        })
    }

    /// The source code of a file.
    fn source(&'a self, id: FileId) -> CodespanResult<Self::Source> {
        Ok(self.lookup(id))
    }

    /// See [`codespan_reporting::files::Files::line_index`].
    fn line_index(&'a self, id: FileId, given: usize) -> CodespanResult<usize> {
        let source = self.lookup(id);
        source
            .byte_to_line(given)
            .ok_or_else(|| CodespanError::IndexTooLarge {
                given,
                max: source.len_bytes(),
            })
    }

    /// See [`codespan_reporting::files::Files::column_number`].
    fn column_number(&'a self, id: FileId, _: usize, given: usize) -> CodespanResult<usize> {
        let source = self.lookup(id);
        source.byte_to_column(given).ok_or_else(|| {
            let max = source.len_bytes();
            if given <= max {
                CodespanError::InvalidCharBoundary { given }
            } else {
                CodespanError::IndexTooLarge { given, max }
            }
        })
    }

    /// See [`codespan_reporting::files::Files::line_range`].
    fn line_range(&'a self, id: FileId, given: usize) -> CodespanResult<std::ops::Range<usize>> {
        self.map_source_or_default(id, 0..0, |source| {
            source
                .line_to_range(given)
                .ok_or_else(|| CodespanError::LineTooLarge {
                    given,
                    max: source.len_lines(),
                })
        })
    }
}

#[comemo::memoize]
fn create_library(inputs: Arc<LazyHash<Dict>>) -> Arc<LazyHash<Library>> {
    let lib = typst::Library::builder()
        .with_inputs(inputs.deref().deref().clone())
        .build();

    Arc::new(LazyHash::new(lib))
}
