use std::{
    num::NonZeroUsize,
    ops::Deref,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, OnceLock},
};

use chrono::{DateTime, Datelike, Local};
use parking_lot::{Mutex, RwLock};
use reflexo_typst::vfs::{notify::FilesystemEvent, Vfs};
use reflexo_typst::ImmutPath;
use reflexo_typst::{error::prelude::*, source::SourceDb};
use salsa::{Durability, Setter};
use typst::{
    diag::{eco_format, At, EcoString, FileError, FileResult, SourceResult},
    engine::{Route, Sink, Traced},
    foundations::{Bytes, Datetime, Dict, Module},
    syntax::{FileId, Source, Span, VirtualPath},
    text::{Font, FontBook},
    utils::LazyHash,
    Library, World,
};

use crate::{ColorTheme, CompilerFeat, LspWorld, SalsaFile, SalsaSource};
use reflexo_typst::world::{
    entry::{EntryManager, EntryReader, EntryState, DETACHED_ENTRY},
    font::FontResolver,
    package::{PackageRegistry, PackageSpec},
    parser::{
        get_semantic_tokens_full, get_semantic_tokens_legend, OffsetEncoding, SemanticToken,
        SemanticTokensLegend,
    },
    source::{SharedState, SourceCache},
    ShadowApi, WorldDeps,
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
        &mut self.inner.local.vfs
    }

    /// Let the vfs notify the access model with a filesystem event.
    ///
    /// See `reflexo_vfs::NotifyAccessModel` for more information.
    pub fn notify_fs_event(&mut self, event: FilesystemEvent) {
        match &event {
            FilesystemEvent::UpstreamUpdate { changeset, .. }
            | FilesystemEvent::Update(changeset) => {
                self.annotate_update(changeset);
            }
        }

        self.inner.local.vfs.notify_fs_event(event);
    }

    pub fn reset_shadow(&mut self) {
        self.annotate_updates(
            self.inner
                .local
                .vfs
                .shadow_paths()
                .iter()
                .map(|p| p.as_ref()),
        );
        self.inner.local.vfs.reset_shadow()
    }

    pub fn map_shadow(&mut self, path: &Path, content: Bytes) -> FileResult<()> {
        self.annotate_updates(std::iter::once(path));
        self.inner.local.vfs.map_shadow(path, content)
    }

    pub fn unmap_shadow(&mut self, path: &Path) -> FileResult<()> {
        self.annotate_updates(std::iter::once(path));
        self.inner.local.vfs.remove_shadow(path);
        Ok(())
    }

    /// Set the `do_reparse` flag.
    pub fn set_do_reparse(&mut self, do_reparse: bool) {
        self.inner.local.base.source_db.do_reparse = do_reparse;
    }

    /// Set the inputs for the compiler.
    pub fn set_inputs(&mut self, inputs: Arc<LazyHash<Dict>>) {
        self.inner.local.inputs = inputs;
    }

    pub fn set_entry_file(&mut self, entry_file: Arc<Path>) -> SourceResult<()> {
        self.inner.set_entry_file_(entry_file)
    }

    pub fn mutate_entry(&mut self, state: EntryState) -> SourceResult<EntryState> {
        self.inner.mutate_entry_(state)
    }

    fn annotate_update(&mut self, changeset: &reflexo_typst::vfs::notify::FileChangeSet) {
        self.annotate_updates(
            changeset
                .inserts
                .iter()
                .map(|(p, _)| p.as_ref())
                .chain(changeset.removes.iter().map(|p| p.as_ref())),
        );
    }

    fn annotate_updates<'a>(&mut self, paths: impl Iterator<Item = &'a Path>) {
        log::info!("annotate_updates ???????");
        log::info!("annotate_updates ??????? 2");
        let revision = self.revision.get();
        for path in paths {
            log::info!("annotated update for path: {path:?} with revision: {revision} before");
            let id = self.vfs().file_id(path);
            log::info!(
                "annotated update for path: {path:?} [{id:?}] with revision: {revision} before 2"
            );
            let source = SalsaFile::new(&self.local, id);
            log::info!(
                "annotated update for path: {path:?} [{id:?}] with revision: {revision} before 3"
            );
            let du = if self
                .local
                .entry
                .root()
                .map_or(true, |root| path.starts_with(root))
            {
                Durability::LOW
            } else {
                Durability::HIGH
            };
            source
                .set_revision(&mut self.local)
                .with_durability(du)
                .to(revision);
            log::info!("annotated update for path: {path:?} [{id:?}] with revision: {revision}");
        }
    }
}

/// A universe that provides access to the operating system.
///
/// Use [`CompilerUniverse::new`] to create a new universe.
/// Use [`CompilerUniverse::snapshot`] to create a new world.
pub struct CompilerUniverse<F: CompilerFeat> {
    // analysis_config: self.analysis_config.clone(),
    pub analysis_config: AnalysisConfig,
    /// The current revision of the source database.
    local: CompilerWorldLocal<F>,
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
        analysis_config: AnalysisConfig,
        inputs: Option<Arc<LazyHash<Dict>>>,
        vfs: Vfs<F::AccessModel>,
        registry: F::Registry,
        font_resolver: Arc<F::FontResolver>,
    ) -> Self {
        let base = CompilerWorld {
            entry,
            library: create_library(inputs.clone().unwrap_or_default()),
            inputs: inputs.unwrap_or_default(),
            font_resolver,
            registry: Arc::new(registry),
            vfs,
            source_db: SourceDb {
                revision: NonZeroUsize::new(1).expect("initial revision is 1"),
                do_reparse: true,
                shared: Default::default(),
                slots: Default::default(),
            },
            now: OnceLock::new(),
        };
        Self {
            analysis_config,

            revision: RwLock::new(NonZeroUsize::new(1).expect("initial revision is 1")),
            shared: Arc::new(RwLock::new(SharedState::default())),

            local: CompilerWorldLocal {
                storage: Default::default(),
                analysis_config: Default::default(),
                base,
                logs: Default::default(),
            },
        }
    }

    /// Wrap driver with a given entry file.
    pub fn with_entry_file(mut self, entry_file: PathBuf) -> Self {
        let _ = self.increment_revision(|this| this.set_entry_file_(entry_file.as_path().into()));
        self
    }

    pub fn do_reparse(&self) -> bool {
        self.local.base.source_db.do_reparse
    }

    pub fn inputs(&self) -> Arc<LazyHash<Dict>> {
        self.local.inputs.clone()
    }

    pub fn snapshot(&self) -> CompilerWorldLocal<F> {
        self.snapshot_with(None)
    }

    pub fn snapshot_with(&self, mutant: Option<TaskInputs>) -> CompilerWorldLocal<F> {
        let rev_lock = self.revision.read();

        log::info!("snapshot_with_locked: {rev_lock:?}");
        let w = mutant
            .map(|m| self.local.task(m))
            .unwrap_or_else(|| self.local.clone());
        log::info!("snapshot_with_locked: {rev_lock:?} 2");

        w
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
        std::mem::swap(&mut self.local.entry, &mut state);
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

    pub fn registry(&self) -> &Arc<F::Registry> {
        &self.local.base.registry
    }
}

impl<F: CompilerFeat> CompilerUniverse<F> {
    /// Reset the world for a new lifecycle (of garbage collection).
    pub fn reset(&mut self) {
        self.local.vfs.reset();
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
            Some(spec) => self.local.registry.resolve(spec)?,
            None => self
                .local
                .entry
                .root()
                .ok_or(FileError::Other(Some(eco_format!(
                    "cannot access directory without root: state: {:?}",
                    self.local.entry
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
        self.local.vfs.shadow_paths()
    }

    #[inline]
    fn reset_shadow(&mut self) {
        self.increment_revision(|this| this.vfs().reset_shadow())
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
        self.local.entry.clone()
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

#[derive(Debug, Clone, Default)]
pub struct AnalysisConfig {
    /// The preferred color theme
    pub color_theme: Option<ColorTheme>,
    /// Remove xxxx todo
    pub remove_html: bool,
}

#[salsa::db]
pub struct CompilerWorldLocal<F>
where
    F: CompilerFeat,
{
    // pub world: Arc<LspWorld>,
    storage: salsa::Storage<Self>,
    pub analysis_config: AnalysisConfig,
    pub base: CompilerWorld<F>,
    pub logs: Arc<Mutex<Option<Vec<String>>>>,
}

impl<F: CompilerFeat> std::ops::Deref for CompilerWorldLocal<F> {
    type Target = CompilerWorld<F>;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl<F: CompilerFeat> std::ops::DerefMut for CompilerWorldLocal<F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

// The analysis db
// analysis_db: AnalysisSourceDb,
use reflexo_typst::TypstFileId;
impl<F: CompilerFeat> PathResolver for CompilerWorldLocal<F> {
    fn file_id_by_path(&self, path: &Path) -> FileResult<TypstFileId> {
        // todo: source in packages
        let root = self.workspace_root().ok_or_else(|| {
            let reason = eco_format!("workspace root not found");
            FileError::Other(Some(reason))
        })?;
        let relative_path = path.strip_prefix(&root).map_err(|_| {
            let reason = eco_format!("access denied, path: {path:?}, root: {root:?}");
            FileError::Other(Some(reason))
        })?;

        Ok(TypstFileId::new(None, VirtualPath::new(relative_path)))
    }
}

impl<F: CompilerFeat> salsa::Database for CompilerWorldLocal<F> {
    fn zalsa_db(&self) {}

    fn salsa_event(&self, event: &dyn Fn() -> salsa::Event) {
        let event = event();
        // Log interesting events, if logging is enabled
        if let Some(logs) = &mut *self.logs.lock() {
            log::debug!("Event: {event:?}");
            // only log interesting events
            if let salsa::EventKind::WillExecute { .. } = event.kind {
                logs.push(format!("Event: {event:?}"));
            }
        }
    }
}

impl Db for LspWorld {
    fn zalsa_db(&self) {}

    fn world(&self) -> &LspWorld {
        self
    }

    fn source_by_id(&self, fid: TypstFileId) -> FileResult<SalsaSource> {
        // todo: we must always touch the revision otherwise it won't be updated
        let path = self.path_for_id(fid)?;
        let id = self.vfs.file_id(&path);
        let _ = SalsaFile::new(self, id).revision(self);

        Ok(SalsaSource::new(self, fid, self.base.source(fid)))
    }
}

impl<F: CompilerFeat> Clone for CompilerWorldLocal<F> {
    fn clone(&self) -> Self {
        self.task(TaskInputs::default())
    }
}

impl<F: CompilerFeat> CompilerWorldLocal<F> {
    pub fn task(&self, mutant: TaskInputs) -> CompilerWorldLocal<F> {
        CompilerWorldLocal {
            storage: Default::default(),
            analysis_config: self.analysis_config.clone(),
            base: self.base.task(mutant),
            logs: self.logs.clone(),
        }
    }

    /// Get a module by string.
    pub fn module_by_str(&self, rr: String) -> Option<Module> {
        let src = Source::new(*DETACHED_ENTRY, rr);
        self.module_by_src(src).ok()
    }

    /// Get (Create) a module by source.
    pub fn module_by_src(&self, source: Source) -> SourceResult<Module> {
        use comemo::Track;
        let route = Route::default();
        let traced = Traced::default();
        let mut sink = Sink::default();

        typst::eval::eval(
            (&self.base as &dyn World).track(),
            traced.track(),
            sink.track_mut(),
            route.track(),
            &source,
        )
    }

    /// Remove html tags from markup content if necessary.
    pub fn remove_html(&self, markup: EcoString) -> EcoString {
        if !self.analysis_config.remove_html {
            return markup;
        }

        static REMOVE_HTML_COMMENT_REGEX: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r#"<!--[\s\S]*?-->"#).unwrap());
        REMOVE_HTML_COMMENT_REGEX
            .replace_all(&markup, "")
            .trim()
            .into()
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

// impl<F: CompilerFeat> Drop for CompilerWorld<F> {
//     fn drop(&mut self) {
//         let state = self.source_db.shared.clone();
//         let source_state = self.source_db.take_state();
//         let mut state = state.write();
//         source_state.commit_impl(&mut state);
//     }
// }

pub use reflexo_typst::world::TaskInputs;

use crate::{Db, PathResolver};

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
    // todo: we need revision for this
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
        let now = self.now.get_or_init(chrono::Local::now);

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
