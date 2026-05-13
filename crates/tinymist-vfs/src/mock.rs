//! Mock VFS support for Tinymist tests.
//!
//! This module intentionally lives in `tinymist-vfs` so VFS tests can use it
//! without depending on higher-level crates. Enable the `mock` feature from
//! downstream test-support crates when this module is needed as a dependency.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use typst::{
    diag::{FileError, FileResult},
    foundations::Bytes,
    syntax::VirtualPath,
};

use crate::{
    FileChangeSet, FileId, FileSnapshot, FilesystemEvent, ImmutPath, MemoryEvent, PathAccessModel,
    RootResolver, Vfs, WorkspaceResolver,
};

type SharedFiles = Arc<RwLock<BTreeMap<PathBuf, FileSnapshot>>>;

/// Path access over a shared in-memory workspace.
///
/// Clones of this type read the same backing map, so tests can mutate a
/// [`MockWorkspace`] and then deliver the corresponding change event to an
/// existing VFS.
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

/// A root resolver for mock VFS tests.
#[derive(Debug, Default)]
pub struct MockRootResolver;

impl RootResolver for MockRootResolver {
    fn resolve_package_root(
        &self,
        _pkg: &typst::syntax::package::PackageSpec,
    ) -> FileResult<ImmutPath> {
        Err(FileError::AccessDenied)
    }
}

/// A deterministic in-memory workspace for VFS and runtime tests.
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

    /// Creates path access for a Tinymist VFS.
    pub fn access_model(&self) -> MockPathAccess {
        MockPathAccess {
            files: self.files.clone(),
        }
    }

    /// Creates a VFS backed by this workspace.
    pub fn vfs(&self) -> Vfs<MockPathAccess> {
        Vfs::new(Arc::new(MockRootResolver), self.access_model())
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
}

/// Returns the default root used by mock workspaces.
pub fn default_mock_root() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\tinymist-mock")
    } else {
        PathBuf::from("/tinymist-mock")
    }
}

fn snapshot(bytes: Bytes) -> FileSnapshot {
    Ok(bytes).into()
}

fn immut_path(path: PathBuf) -> ImmutPath {
    Arc::from(path.into_boxed_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_workspace_drives_vfs_updates() {
        let workspace = MockWorkspace::default_builder()
            .file("main.typ", "#let value = [before]\n#value")
            .build();
        let mut vfs = workspace.vfs();
        let main_id = workspace.file_id("main.typ").unwrap();

        assert_eq!(
            vfs.source(main_id).unwrap().text(),
            "#let value = [before]\n#value"
        );

        workspace
            .update_source("main.typ", "#let value = [after]\n#value")
            .apply_to_vfs(&mut vfs);

        assert_eq!(
            vfs.source(main_id).unwrap().text(),
            "#let value = [after]\n#value"
        );
    }
}
