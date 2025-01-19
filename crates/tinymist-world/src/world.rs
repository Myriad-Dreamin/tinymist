use std::{
    num::NonZeroUsize,
    ops::Deref,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, OnceLock},
};

use chrono::{DateTime, Datelike, Local};
use parking_lot::RwLock;
use tinymist_std::error::prelude::*;
use tinymist_std::ImmutPath;
use tinymist_vfs::{notify::FilesystemEvent, Vfs};
use typst::{
    diag::{eco_format, At, EcoString, FileError, FileResult, SourceResult},
    foundations::{Bytes, Datetime, Dict},
    syntax::{FileId, Source, Span},
    text::{Font, FontBook},
    utils::LazyHash,
    Library, World,
};

use crate::source::{SharedState, SourceCache, SourceDb};
use crate::{
    entry::{EntryManager, EntryReader, EntryState, DETACHED_ENTRY},
    font::FontResolver,
    package::{PackageRegistry, PackageSpec},
    parser::{
        get_semantic_tokens_full, get_semantic_tokens_legend, OffsetEncoding, SemanticToken,
        SemanticTokensLegend,
    },
    CompilerFeat, ShadowApi, WorldDeps,
};

type CodespanResult<T> = Result<T, CodespanError>;
type CodespanError = codespan_reporting::files::Error;

pub struct Revising<'a, T> {
    pub revision: NonZeroUsize,
    pub inner: &'a mut T,
}

impl<T> std::ops::Deref for Revising<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<T> std::ops::DerefMut for Revising<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

impl<F: CompilerFeat> Revising<'_, CompilerUniverse<F>> {
    pub fn vfs(&mut self) -> &mut Vfs<F::AccessModel> {
        &mut self.inner.vfs
    }

    /// Let the vfs notify the access model with a filesystem event.
    ///
    /// See `reflexo_vfs::NotifyAccessModel` for more information.
    pub fn notify_fs_event(&mut self, event: FilesystemEvent) {
        self.inner.vfs.notify_fs_event(event);
    }

    pub fn reset_shadow(&mut self) {
        self.inner.vfs.reset_shadow()
    }

    pub fn map_shadow(&mut self, path: &Path, content: Bytes) -> FileResult<()> {
        self.inner.vfs.map_shadow(path, content)
    }

    pub fn unmap_shadow(&mut self, path: &Path) -> FileResult<()> {
        self.inner.vfs.remove_shadow(path);
        Ok(())
    }

    /// Set the inputs for the compiler.
    pub fn set_inputs(&mut self, inputs: Arc<LazyHash<Dict>>) {
        self.inner.inputs = inputs;
    }

    pub fn set_entry_file(&mut self, entry_file: Arc<Path>) -> SourceResult<()> {
        self.inner.set_entry_file_(entry_file)
    }

    pub fn mutate_entry(&mut self, state: EntryState) -> SourceResult<EntryState> {
        self.inner.mutate_entry_(state)
    }
}

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

    /// The current revision of the source database.
    pub revision: RwLock<NonZeroUsize>,
    /// Shared state for source cache.
    pub shared: Arc<RwLock<SharedState<SourceCache>>>,
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
        registry: F::Registry,
        font_resolver: Arc<F::FontResolver>,
    ) -> Self {
        Self {
            entry,
            inputs: inputs.unwrap_or_default(),

            revision: RwLock::new(NonZeroUsize::new(1).expect("initial revision is 1")),
            shared: Arc::new(RwLock::new(SharedState::default())),

            font_resolver,
            registry: Arc::new(registry),
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
        let rev_lock = self.revision.read();

        let w = CompilerWorld {
            entry: self.entry.clone(),
            inputs: self.inputs.clone(),
            library: create_library(self.inputs.clone()),
            font_resolver: self.font_resolver.clone(),
            registry: self.registry.clone(),
            vfs: self.vfs.snapshot(),
            source_db: SourceDb {
                revision: *rev_lock,
                shared: self.shared.clone(),
                slots: Default::default(),
            },
            now: OnceLock::new(),
        };

        mutant.map(|m| w.task(m)).unwrap_or(w)
    }

    /// Increment revision with actions.
    pub fn increment_revision<T>(&mut self, f: impl FnOnce(&mut Revising<Self>) -> T) -> T {
        let rev_lock = self.revision.get_mut();
        *rev_lock = rev_lock.checked_add(1).unwrap();
        let revision = *rev_lock;
        f(&mut Revising {
            inner: self,
            revision,
        })
    }

    /// Mutate the entry state and return the old state.
    fn mutate_entry_(&mut self, mut state: EntryState) -> SourceResult<EntryState> {
        self.reset();
        std::mem::swap(&mut self.entry, &mut state);
        Ok(state)
    }

    /// set an entry file.
    fn set_entry_file_(&mut self, entry_file: Arc<Path>) -> SourceResult<()> {
        let state = self.entry_state();
        let state = state
            .try_select_path_in_workspace(&entry_file, true)
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
        self.vfs.reset();
        // todo: shared state
    }

    /// Resolve the real path for a file id.
    pub fn path_for_id(&self, id: FileId) -> Result<PathBuf, FileError> {
        if id == *DETACHED_ENTRY {
            return Ok(DETACHED_ENTRY.vpath().as_rooted_path().to_owned());
        }

        // Determine the root path relative to which the file path
        // will be resolved.
        let root = match id.package() {
            Some(spec) => self.registry.resolve(spec)?,
            None => self.entry.root().ok_or(FileError::Other(Some(eco_format!(
                "cannot access directory without root: state: {:?}",
                self.entry
            ))))?,
        };

        // Join the path to the root. If it tries to escape, deny
        // access. Note: It can still escape via symlinks.
        id.vpath().resolve(&root).ok_or(FileError::AccessDenied)
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
                    .try_select_path_in_workspace(path, true)?
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
    fn _shadow_map_id(&self, file_id: FileId) -> FileResult<PathBuf> {
        self.path_for_id(file_id)
    }

    #[inline]
    fn shadow_paths(&self) -> Vec<Arc<Path>> {
        self.vfs.shadow_paths()
    }

    #[inline]
    fn reset_shadow(&mut self) {
        self.increment_revision(|this| this.vfs.reset_shadow())
    }

    #[inline]
    fn map_shadow(&mut self, path: &Path, content: Bytes) -> FileResult<()> {
        self.increment_revision(|this| this.vfs().map_shadow(path, content))
    }

    #[inline]
    fn unmap_shadow(&mut self, path: &Path) -> FileResult<()> {
        self.increment_revision(|this| {
            this.vfs().remove_shadow(path);
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
    fn reset(&mut self) -> SourceResult<()> {
        self.reset();
        Ok(())
    }

    fn mutate_entry(&mut self, state: EntryState) -> SourceResult<EntryState> {
        self.increment_revision(|this| this.mutate_entry_(state))
    }
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

    /// Provides source database for typst compiler.
    pub source_db: SourceDb,
    /// The current datetime if requested. This is stored here to ensure it is
    /// always the same within one compilation. Reset between compilations.
    now: OnceLock<DateTime<Local>>,
}

impl<F: CompilerFeat> Clone for CompilerWorld<F> {
    fn clone(&self) -> Self {
        self.task(TaskInputs::default())
    }
}

impl<F: CompilerFeat> Drop for CompilerWorld<F> {
    fn drop(&mut self) {
        let state = self.source_db.shared.clone();
        let source_state = self.source_db.take_state();
        let mut state = state.write();
        source_state.commit_impl(&mut state);
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

        CompilerWorld {
            inputs: mutant.inputs.unwrap_or_else(|| self.inputs.clone()),
            library: library.unwrap_or_else(|| self.library.clone()),
            entry: mutant.entry.unwrap_or_else(|| self.entry.clone()),
            font_resolver: self.font_resolver.clone(),
            registry: self.registry.clone(),
            vfs: self.vfs.snapshot(),
            source_db: self.source_db.clone(),
            now: self.now.clone(),
        }
    }

    pub fn inputs(&self) -> Arc<LazyHash<Dict>> {
        self.inputs.clone()
    }

    /// Resolve the real path for a file id.
    pub fn path_for_id(&self, id: FileId) -> Result<PathBuf, FileError> {
        if id == *DETACHED_ENTRY {
            return Ok(DETACHED_ENTRY.vpath().as_rooted_path().to_owned());
        }

        // Determine the root path relative to which the file path
        // will be resolved.
        let root = match id.package() {
            Some(spec) => self.registry.resolve(spec)?,
            None => self.entry.root().ok_or(FileError::Other(Some(eco_format!(
                "cannot access directory without root: state: {:?}",
                self.entry
            ))))?,
        };

        // Join the path to the root. If it tries to escape, deny
        // access. Note: It can still escape via symlinks.
        id.vpath().resolve(&root).ok_or(FileError::AccessDenied)
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
        self.source_db.revision
    }
}

impl<F: CompilerFeat> ShadowApi for CompilerWorld<F> {
    #[inline]
    fn _shadow_map_id(&self, file_id: FileId) -> FileResult<PathBuf> {
        self.path_for_id(file_id)
    }

    #[inline]
    fn shadow_paths(&self) -> Vec<Arc<Path>> {
        self.vfs.shadow_paths()
    }

    #[inline]
    fn reset_shadow(&mut self) {
        self.vfs.reset_shadow()
    }

    #[inline]
    fn map_shadow(&mut self, path: &Path, content: Bytes) -> FileResult<()> {
        self.vfs.map_shadow(path, content)
    }

    #[inline]
    fn unmap_shadow(&mut self, path: &Path) -> FileResult<()> {
        self.vfs.remove_shadow(path);
        Ok(())
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

        let fid = self.vfs.file_id(&self.path_for_id(id)?);
        self.source_db.source(id, fid, &self.vfs)
    }

    /// Try to access the specified file.
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        let fid = self.vfs.file_id(&self.path_for_id(id)?);
        self.source_db.file(id, fid, &self.vfs)
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
    fn iter_dependencies(&self, f: &mut dyn FnMut(ImmutPath)) {
        self.source_db.iter_dependencies_dyn(&self.vfs, f)
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
        let vpath = id.vpath();
        Ok(if let Some(package) = id.package() {
            format!("{package}{}", vpath.as_rooted_path().display())
        } else {
            match self.entry.root() {
                Some(root) => {
                    // Try to express the path relative to the working directory.
                    vpath
                        .resolve(&root)
                        // differ from typst
                        // .and_then(|abs| pathdiff::diff_paths(&abs, self.workdir()))
                        .as_deref()
                        .unwrap_or_else(|| vpath.as_rootless_path())
                        .to_string_lossy()
                        .into()
                }
                None => vpath.as_rooted_path().display().to_string(),
            }
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
