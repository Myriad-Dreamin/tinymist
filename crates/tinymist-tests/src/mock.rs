//! Mock-backed VFS, world, and project test harnesses.
//!
//! Use [`MockWorkspace`] and [`MockPathAccess`] for VFS-only tests that only
//! need path-based file reads and explicit [`FileChangeSet`] invalidation.
//! Use [`MockWorldBuilder`] for world-level tests that need a deterministic
//! [`CompilerUniverse`] or [`CompilerWorld`] backed by embedded fonts and a
//! dummy package registry. Use [`MockWorldBuilder::project_compiler`] for
//! project-level tests that should drive the same [`FilesystemEvent`] or
//! [`MemoryEvent`] flow as runtime code.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, RwLock},
};

use tinymist_project::{CompileServerOpts, Interrupt, ProjectCompiler};
use tinymist_std::ImmutPath;
use tinymist_vfs::{
    FileChangeSet, FileId, FileSnapshot, FilesystemEvent, MemoryEvent, PathAccessModel,
    RootResolver, Vfs, WorkspaceResolver, notify::NotifyMessage,
};
use tinymist_world::{
    CompilerFeat, CompilerUniverse, CompilerWorld, EntryState,
    font::{FontResolverImpl, memory::MemoryFontSearcher},
    package::{RegistryPathMapper, registry::DummyRegistry},
};
use tokio::sync::mpsc;
use typst::{
    Features,
    diag::{FileError, FileResult},
    foundations::{Bytes, Dict},
    syntax::VirtualPath,
    utils::LazyHash,
};

type SharedFiles = Arc<RwLock<BTreeMap<PathBuf, FileSnapshot>>>;

/// A compiler feature set for mock-backed Tinymist worlds.
#[derive(Debug, Clone, Copy)]
pub struct MockCompilerFeat;

impl CompilerFeat for MockCompilerFeat {
    type FontResolver = FontResolverImpl;
    type AccessModel = MockPathAccess;
    type Registry = DummyRegistry;
}

/// A compiler universe backed by [`MockWorkspace`].
pub type MockUniverse = CompilerUniverse<MockCompilerFeat>;

/// A compiler world backed by [`MockWorkspace`].
pub type MockWorld = CompilerWorld<MockCompilerFeat>;

/// A project compiler backed by [`MockWorkspace`].
pub type MockProjectCompiler<Ext = ()> = ProjectCompiler<MockCompilerFeat, Ext>;

/// Path access over a shared in-memory workspace.
///
/// Clones of this type all read the same backing map, so a test can mutate a
/// [`MockWorkspace`] and then deliver the corresponding change event to an
/// existing VFS or universe.
#[derive(Debug, Clone)]
pub struct MockPathAccess {
    files: SharedFiles,
}

impl PathAccessModel for MockPathAccess {
    fn content(&self, src: &Path) -> FileResult<Bytes> {
        self.files
            .read()
            .expect("mock workspace lock poisoned")
            .get(src)
            .ok_or_else(|| FileError::NotFound(src.into()))
            .and_then(|snapshot| snapshot.content().cloned())
    }
}

/// A deterministic in-memory workspace for Tinymist runtime tests.
///
/// Paths accepted by this type may be relative to the workspace root or already
/// absolute. File writes are upserts because Tinymist's runtime-facing
/// [`FileChangeSet`] insert side also represents both creates and updates.
#[derive(Debug, Clone)]
pub struct MockWorkspace {
    root: PathBuf,
    files: SharedFiles,
}

impl Default for MockWorkspace {
    fn default() -> Self {
        Self::new(default_mock_root())
    }
}

impl MockWorkspace {
    /// Creates an empty mock workspace at the given root.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            files: Arc::default(),
        }
    }

    /// Creates a builder for a mock workspace at the given root.
    pub fn builder(root: impl Into<PathBuf>) -> MockWorkspaceBuilder {
        MockWorkspaceBuilder::new(root)
    }

    /// Creates a builder for a mock workspace at the default test root.
    pub fn default_builder() -> MockWorkspaceBuilder {
        MockWorkspaceBuilder::new(default_mock_root())
    }

    /// Returns the workspace root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the workspace root as an immutable path.
    pub fn root_path(&self) -> ImmutPath {
        immut_path(self.root.clone())
    }

    /// Resolves a test path against the workspace root.
    pub fn path(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();
        if path.is_absolute() {
            path.to_owned()
        } else {
            self.root.join(path)
        }
    }

    /// Resolves a test path against the workspace root as an immutable path.
    pub fn immut_path(&self, path: impl AsRef<Path>) -> ImmutPath {
        immut_path(self.path(path))
    }

    /// Resolves a test path as a Typst virtual path inside the workspace.
    pub fn virtual_path(&self, path: impl AsRef<Path>) -> FileResult<VirtualPath> {
        let path = self.path(path);
        let relative = path
            .strip_prefix(&self.root)
            .map_err(|_| FileError::AccessDenied)?;
        Ok(VirtualPath::new(relative))
    }

    /// Resolves a test path as a workspace [`FileId`].
    pub fn file_id(&self, path: impl AsRef<Path>) -> FileResult<FileId> {
        Ok(WorkspaceResolver::workspace_file(
            Some(&self.root_path()),
            self.virtual_path(path)?,
        ))
    }

    /// Creates an entry state rooted at this workspace.
    pub fn entry_state(&self, entry: impl AsRef<Path>) -> FileResult<EntryState> {
        Ok(EntryState::new_rooted(
            self.root_path(),
            Some(self.virtual_path(entry)?),
        ))
    }

    /// Creates path access for a Tinymist VFS.
    pub fn access_model(&self) -> MockPathAccess {
        MockPathAccess {
            files: self.files.clone(),
        }
    }

    /// Creates a VFS backed by this workspace.
    pub fn vfs(&self) -> Vfs<MockPathAccess> {
        let registry = Arc::new(DummyRegistry);
        let resolver: Arc<dyn RootResolver + Send + Sync> =
            Arc::new(RegistryPathMapper::new(registry));
        Vfs::new(resolver, self.access_model())
    }

    /// Creates a world builder for this workspace and entry file.
    pub fn world(&self, entry: impl Into<PathBuf>) -> MockWorldBuilder {
        MockWorldBuilder::new(self.clone(), entry)
    }

    /// Reads bytes from the in-memory workspace.
    pub fn read(&self, path: impl AsRef<Path>) -> FileResult<Bytes> {
        let path = self.path(path);
        self.files
            .read()
            .expect("mock workspace lock poisoned")
            .get(&path)
            .ok_or_else(|| FileError::NotFound(path.clone()))
            .and_then(|snapshot| snapshot.content().cloned())
    }

    /// Returns whether a file exists in the in-memory workspace.
    pub fn contains(&self, path: impl AsRef<Path>) -> bool {
        self.files
            .read()
            .expect("mock workspace lock poisoned")
            .contains_key(&self.path(path))
    }

    /// Creates or updates a Typst source file.
    pub fn write_source(&self, path: impl AsRef<Path>, source: impl Into<String>) -> MockChange {
        self.write_bytes(path, Bytes::from_string(source.into()))
    }

    /// Creates a Typst source file.
    pub fn create_source(&self, path: impl AsRef<Path>, source: impl Into<String>) -> MockChange {
        self.write_source(path, source)
    }

    /// Updates a Typst source file.
    pub fn update_source(&self, path: impl AsRef<Path>, source: impl Into<String>) -> MockChange {
        self.write_source(path, source)
    }

    /// Creates or updates a file with arbitrary bytes.
    pub fn write_bytes(&self, path: impl AsRef<Path>, bytes: Bytes) -> MockChange {
        let path = self.path(path);
        let snapshot = snapshot(bytes);

        self.files
            .write()
            .expect("mock workspace lock poisoned")
            .insert(path.clone(), snapshot.clone());

        MockChange::new(FileChangeSet::new_inserts(vec![(
            immut_path(path),
            snapshot,
        )]))
    }

    /// Removes a file from the in-memory workspace.
    pub fn remove(&self, path: impl AsRef<Path>) -> FileResult<MockChange> {
        let path = self.path(path);

        let removed = self
            .files
            .write()
            .expect("mock workspace lock poisoned")
            .remove(&path);

        match removed {
            Some(_) => Ok(MockChange::new(FileChangeSet::new_removes(vec![
                immut_path(path),
            ]))),
            None => Err(FileError::NotFound(path)),
        }
    }

    /// Renames a file inside the in-memory workspace.
    pub fn rename(&self, from: impl AsRef<Path>, to: impl AsRef<Path>) -> FileResult<MockChange> {
        let from = self.path(from);
        let to = self.path(to);

        let mut files = self.files.write().expect("mock workspace lock poisoned");
        let snapshot = files
            .remove(&from)
            .ok_or_else(|| FileError::NotFound(from.clone()))?;
        files.insert(to.clone(), snapshot.clone());

        Ok(MockChange::new(FileChangeSet {
            removes: vec![immut_path(from)],
            inserts: vec![(immut_path(to), snapshot)],
        }))
    }

    /// Returns a changeset that syncs the current workspace files.
    pub fn sync_changeset(&self) -> FileChangeSet {
        let inserts = self
            .files
            .read()
            .expect("mock workspace lock poisoned")
            .iter()
            .map(|(path, snapshot)| (immut_path(path.clone()), snapshot.clone()))
            .collect();

        FileChangeSet::new_inserts(inserts)
    }

    /// Returns a filesystem event that syncs the current workspace files.
    pub fn sync_filesystem_event(&self) -> FilesystemEvent {
        FilesystemEvent::Update(self.sync_changeset(), true)
    }

    /// Returns a memory event that syncs the current workspace files.
    pub fn sync_memory_event(&self) -> MemoryEvent {
        MemoryEvent::Sync(self.sync_changeset())
    }
}

/// Builder for [`MockWorkspace`].
#[derive(Debug)]
pub struct MockWorkspaceBuilder {
    workspace: MockWorkspace,
}

impl MockWorkspaceBuilder {
    /// Creates a mock workspace builder at the given root.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            workspace: MockWorkspace::new(root),
        }
    }

    /// Adds a Typst source file to the workspace.
    pub fn file(self, path: impl AsRef<Path>, source: impl Into<String>) -> Self {
        self.workspace.write_source(path, source);
        self
    }

    /// Adds an arbitrary byte file to the workspace.
    pub fn bytes(self, path: impl AsRef<Path>, bytes: Bytes) -> Self {
        self.workspace.write_bytes(path, bytes);
        self
    }

    /// Finishes the builder.
    pub fn build(self) -> MockWorkspace {
        self.workspace
    }
}

/// A workspace mutation and its runtime-facing changeset.
#[derive(Debug, Clone)]
pub struct MockChange {
    changeset: FileChangeSet,
}

impl MockChange {
    /// Creates a mock change from a changeset.
    pub fn new(changeset: FileChangeSet) -> Self {
        Self { changeset }
    }

    /// Returns the changeset.
    pub fn changeset(&self) -> &FileChangeSet {
        &self.changeset
    }

    /// Consumes this change and returns the changeset.
    pub fn into_changeset(self) -> FileChangeSet {
        self.changeset
    }

    /// Returns this change as a filesystem event.
    pub fn filesystem_event(&self, is_sync: bool) -> FilesystemEvent {
        FilesystemEvent::Update(self.changeset.clone(), is_sync)
    }

    /// Consumes this change and returns it as a filesystem event.
    pub fn into_filesystem_event(self, is_sync: bool) -> FilesystemEvent {
        FilesystemEvent::Update(self.changeset, is_sync)
    }

    /// Returns this change as a memory update event.
    pub fn memory_event(&self) -> MemoryEvent {
        MemoryEvent::Update(self.changeset.clone())
    }

    /// Returns this change as a memory sync event.
    pub fn memory_sync_event(&self) -> MemoryEvent {
        MemoryEvent::Sync(self.changeset.clone())
    }

    /// Applies this change to a VFS through `notify_fs_changes`.
    pub fn apply_to_vfs<M>(&self, vfs: &mut Vfs<M>)
    where
        M: PathAccessModel,
    {
        vfs.revise().notify_fs_changes(self.changeset.clone());
    }

    /// Applies this change to a compiler universe through the VFS revision path.
    pub fn apply_to_universe<F>(&self, universe: &mut CompilerUniverse<F>)
    where
        F: CompilerFeat,
    {
        universe.increment_revision(|universe| {
            universe.vfs().notify_fs_changes(self.changeset.clone());
        });
    }

    /// Applies this change to a project compiler as a filesystem event.
    pub fn apply_as_fs_to_project<F, Ext>(
        &self,
        compiler: &mut ProjectCompiler<F, Ext>,
        is_sync: bool,
    ) where
        F: CompilerFeat + Send + Sync + 'static,
        Ext: Default + 'static,
    {
        compiler.process(Interrupt::Fs(self.filesystem_event(is_sync)));
    }

    /// Applies this change to a project compiler as a memory event.
    pub fn apply_as_memory_to_project<F, Ext>(&self, compiler: &mut ProjectCompiler<F, Ext>)
    where
        F: CompilerFeat + Send + Sync + 'static,
        Ext: Default + 'static,
    {
        compiler.process(Interrupt::Memory(self.memory_event()));
    }
}

/// Builder for mock-backed compiler worlds.
#[derive(Debug, Clone)]
pub struct MockWorldBuilder {
    workspace: MockWorkspace,
    entry: PathBuf,
    features: Features,
    inputs: Option<Arc<LazyHash<Dict>>>,
    font_resolver: Option<Arc<FontResolverImpl>>,
    creation_timestamp: Option<i64>,
}

impl MockWorldBuilder {
    /// Creates a mock world builder.
    pub fn new(workspace: MockWorkspace, entry: impl Into<PathBuf>) -> Self {
        Self {
            workspace,
            entry: entry.into(),
            features: Features::default(),
            inputs: None,
            font_resolver: None,
            creation_timestamp: None,
        }
    }

    /// Sets the Typst feature flags for the universe.
    pub fn with_features(mut self, features: Features) -> Self {
        self.features = features;
        self
    }

    /// Sets Typst input values for the universe.
    pub fn with_inputs(mut self, inputs: Dict) -> Self {
        self.inputs = Some(Arc::new(LazyHash::new(inputs)));
        self
    }

    /// Sets pre-hashed Typst input values for the universe.
    pub fn with_lazy_inputs(mut self, inputs: Arc<LazyHash<Dict>>) -> Self {
        self.inputs = Some(inputs);
        self
    }

    /// Sets the font resolver for the universe.
    pub fn with_font_resolver(mut self, resolver: Arc<FontResolverImpl>) -> Self {
        self.font_resolver = Some(resolver);
        self
    }

    /// Sets a deterministic creation timestamp for the universe.
    pub fn with_creation_timestamp(mut self, timestamp: Option<i64>) -> Self {
        self.creation_timestamp = timestamp;
        self
    }

    /// Builds a compiler universe.
    pub fn build_universe(&self) -> FileResult<MockUniverse> {
        let registry = Arc::new(DummyRegistry);
        let resolver: Arc<dyn RootResolver + Send + Sync> =
            Arc::new(RegistryPathMapper::new(registry.clone()));

        Ok(CompilerUniverse::new_raw(
            self.workspace.entry_state(&self.entry)?,
            self.features.clone(),
            self.inputs.clone(),
            Vfs::new(resolver, self.workspace.access_model()),
            registry,
            self.font_resolver
                .clone()
                .unwrap_or_else(embedded_font_resolver),
            self.creation_timestamp,
        ))
    }

    /// Builds a compiler world snapshot.
    pub fn build_world(&self) -> FileResult<MockWorld> {
        Ok(self.build_universe()?.snapshot())
    }

    /// Builds a syntax-only project compiler and its notify receiver.
    pub fn project_compiler<Ext>(
        &self,
    ) -> FileResult<(
        MockProjectCompiler<Ext>,
        mpsc::UnboundedReceiver<NotifyMessage>,
    )>
    where
        Ext: Default + 'static,
    {
        self.project_compiler_with_opts(CompileServerOpts {
            syntax_only: true,
            ..Default::default()
        })
    }

    /// Builds a project compiler with custom options and its notify receiver.
    pub fn project_compiler_with_opts<Ext>(
        &self,
        opts: CompileServerOpts<MockCompilerFeat, Ext>,
    ) -> FileResult<(
        MockProjectCompiler<Ext>,
        mpsc::UnboundedReceiver<NotifyMessage>,
    )>
    where
        Ext: Default + 'static,
    {
        let (tx, rx) = mpsc::unbounded_channel();
        Ok((ProjectCompiler::new(self.build_universe()?, tx, opts), rx))
    }
}

/// Returns the default root used by mock workspaces.
pub fn default_mock_root() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\tinymist-mock")
    } else {
        PathBuf::from("/tinymist-mock")
    }
}

/// Returns a deterministic font resolver using Typst's embedded fonts.
pub fn embedded_font_resolver() -> Arc<FontResolverImpl> {
    static FONT_RESOLVER: LazyLock<Arc<FontResolverImpl>> = LazyLock::new(|| {
        let mut searcher = MemoryFontSearcher::new();
        for font in typst_assets::fonts() {
            searcher.add_memory_font(Bytes::new(font));
        }
        Arc::new(searcher.build())
    });

    FONT_RESOLVER.clone()
}

fn snapshot(bytes: Bytes) -> FileSnapshot {
    Ok(bytes).into()
}

fn immut_path(path: PathBuf) -> ImmutPath {
    Arc::from(path.into_boxed_path())
}

#[cfg(test)]
mod tests {
    use tinymist_std::typst::TypstPagedDocument;

    use super::*;

    #[test]
    fn builds_world_from_memory_and_applies_update() {
        let workspace = MockWorkspace::default_builder()
            .file("main.typ", "#import \"content.typ\": value\n#value")
            .file("content.typ", "#let value = [before]")
            .build();

        let mut universe = workspace.world("main.typ").build_universe().unwrap();
        let content_path = workspace.path("content.typ");

        let world = universe.snapshot();
        assert_eq!(
            world.source_by_path(&content_path).unwrap().text(),
            "#let value = [before]"
        );
        typst::compile::<TypstPagedDocument>(&world).output.unwrap();

        let change = workspace.update_source("content.typ", "#let value = [after]");
        change.apply_to_universe(&mut universe);

        let world = universe.snapshot();
        assert_eq!(
            world.source_by_path(&content_path).unwrap().text(),
            "#let value = [after]"
        );
    }

    #[test]
    fn rename_remove_flow_updates_runtime_without_rebuilding_workspace() {
        let workspace = MockWorkspace::default_builder()
            .file("main.typ", "#import \"content.typ\": value\n#value")
            .file("content.typ", "#let value = [before]")
            .build();

        let mut universe = workspace.world("main.typ").build_universe().unwrap();
        let content_path = workspace.path("content.typ");
        let renamed_path = workspace.path("renamed.typ");
        let main_path = workspace.path("main.typ");

        assert_eq!(
            universe
                .snapshot()
                .source_by_path(&content_path)
                .unwrap()
                .text(),
            "#let value = [before]"
        );

        workspace
            .rename("content.typ", "renamed.typ")
            .unwrap()
            .apply_to_universe(&mut universe);
        workspace
            .update_source("main.typ", "#import \"renamed.typ\": value\n#value")
            .apply_to_universe(&mut universe);

        let world = universe.snapshot();
        assert!(world.source_by_path(&content_path).is_err());
        assert_eq!(
            world.source_by_path(&renamed_path).unwrap().text(),
            "#let value = [before]"
        );

        workspace
            .remove("renamed.typ")
            .unwrap()
            .apply_to_universe(&mut universe);
        workspace
            .update_source("main.typ", "#let value = [inline]\n#value")
            .apply_to_universe(&mut universe);

        let world = universe.snapshot();
        assert!(world.source_by_path(&renamed_path).is_err());
        assert_eq!(
            world.source_by_path(&main_path).unwrap().text(),
            "#let value = [inline]\n#value"
        );
    }

    #[test]
    fn project_compiler_accepts_mock_filesystem_events() {
        let workspace = MockWorkspace::default_builder()
            .file("main.typ", "#let value = [before]\n#value")
            .build();

        let (mut compiler, _notify_rx) = workspace
            .world("main.typ")
            .project_compiler::<()>()
            .unwrap();

        assert!(!compiler.primary.reason.any());

        workspace
            .update_source("main.typ", "#let value = [after]\n#value")
            .apply_as_fs_to_project(&mut compiler, false);

        assert!(compiler.primary.reason.by_fs_events);
        assert_eq!(
            compiler
                .primary
                .verse
                .snapshot()
                .source_by_path(&workspace.path("main.typ"))
                .unwrap()
                .text(),
            "#let value = [after]\n#value"
        );
    }
}
