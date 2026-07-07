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
        VirtualPath::virtualize(&self.root, &path).map_err(|_| FileError::AccessDenied)
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

    const ENTRY: &str = "main.typ";
    const DEP: &str = "dep.typ";
    const RENAMED_DEP: &str = "renamed.typ";
    const UNRELATED: &str = "notes.typ";
    const ASSET: &str = "image.svg";

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum OperationId {
        O01,
        O02,
        O03,
        O04,
        O05,
        O06,
        O07,
        O08,
        O09,
        O10,
        O11,
        O12,
        O13,
        O14,
        O15,
        O16,
        O17,
        O18,
        O19,
        O20,
    }

    impl OperationId {
        fn label(self) -> &'static str {
            match self {
                OperationId::O01 => "O01",
                OperationId::O02 => "O02",
                OperationId::O03 => "O03",
                OperationId::O04 => "O04",
                OperationId::O05 => "O05",
                OperationId::O06 => "O06",
                OperationId::O07 => "O07",
                OperationId::O08 => "O08",
                OperationId::O09 => "O09",
                OperationId::O10 => "O10",
                OperationId::O11 => "O11",
                OperationId::O12 => "O12",
                OperationId::O13 => "O13",
                OperationId::O14 => "O14",
                OperationId::O15 => "O15",
                OperationId::O16 => "O16",
                OperationId::O17 => "O17",
                OperationId::O18 => "O18",
                OperationId::O19 => "O19",
                OperationId::O20 => "O20",
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum RelationVariant {
        Entry,
        ActiveDependency,
        MissingDependency,
        RetainedInactiveDependency,
        AssetDependency,
        ShadowOpenPath,
        UnrelatedPath,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum CachePostcondition {
        InsertRefreshesCurrentSource,
        SamePathReadErrorReplacesSource,
        RemoveRetiresPath,
        RemoveThenInsertRefreshesPath,
        RenameRetiresOldPath,
        PrefixRewriteRetiresOldPaths,
        RootBoundaryRetiresOldPath,
        NoDirectVfsChange,
        ShadowOverlayOrdersWithFilesystem,
        MixedBatchFinalState,
    }

    impl CachePostcondition {
        fn grouping_note(self) -> &'static str {
            match self {
                CachePostcondition::InsertRefreshesCurrentSource => {
                    "same-path create/update-like rows must refresh cached bytes and parsed Source"
                }
                CachePostcondition::SamePathReadErrorReplacesSource => {
                    "read-error rows must replace the old readable snapshot until recovery"
                }
                CachePostcondition::RemoveRetiresPath => {
                    "remove-like rows must stop serving cached source for the old path"
                }
                CachePostcondition::RemoveThenInsertRefreshesPath => {
                    "delete/recreate rows must observe missing then fresh contents"
                }
                CachePostcondition::RenameRetiresOldPath => {
                    "rename rows must retire the old path and make the new path independently readable"
                }
                CachePostcondition::PrefixRewriteRetiresOldPaths => {
                    "directory-prefix rows must retire every known old child path"
                }
                CachePostcondition::RootBoundaryRetiresOldPath => {
                    "root-boundary rows must retire paths that leave the addressable workspace"
                }
                CachePostcondition::NoDirectVfsChange => {
                    "dependency membership rows with no file delta must not dirty VFS state by themselves"
                }
                CachePostcondition::ShadowOverlayOrdersWithFilesystem => {
                    "shadow-open rows must keep memory content active until the shadow is removed"
                }
                CachePostcondition::MixedBatchFinalState => {
                    "mixed batches are asserted by final observable VFS state, independent of atom order"
                }
            }
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct VfsCacheMatrixRow {
        id: OperationId,
        postcondition: CachePostcondition,
        relations: &'static [RelationVariant],
    }

    const VFS_CACHE_FILE_OPERATION_MATRIX: &[VfsCacheMatrixRow] = &[
        VfsCacheMatrixRow {
            id: OperationId::O01,
            postcondition: CachePostcondition::InsertRefreshesCurrentSource,
            relations: &[
                RelationVariant::MissingDependency,
                RelationVariant::UnrelatedPath,
                RelationVariant::Entry,
            ],
        },
        VfsCacheMatrixRow {
            id: OperationId::O02,
            postcondition: CachePostcondition::InsertRefreshesCurrentSource,
            relations: &[
                RelationVariant::Entry,
                RelationVariant::ActiveDependency,
                RelationVariant::AssetDependency,
                RelationVariant::UnrelatedPath,
            ],
        },
        VfsCacheMatrixRow {
            id: OperationId::O03,
            postcondition: CachePostcondition::InsertRefreshesCurrentSource,
            relations: &[
                RelationVariant::Entry,
                RelationVariant::ActiveDependency,
                RelationVariant::UnrelatedPath,
            ],
        },
        VfsCacheMatrixRow {
            id: OperationId::O04,
            postcondition: CachePostcondition::SamePathReadErrorReplacesSource,
            relations: &[
                RelationVariant::Entry,
                RelationVariant::ActiveDependency,
                RelationVariant::AssetDependency,
            ],
        },
        VfsCacheMatrixRow {
            id: OperationId::O05,
            postcondition: CachePostcondition::RemoveRetiresPath,
            relations: &[
                RelationVariant::Entry,
                RelationVariant::ActiveDependency,
                RelationVariant::RetainedInactiveDependency,
                RelationVariant::UnrelatedPath,
            ],
        },
        VfsCacheMatrixRow {
            id: OperationId::O06,
            postcondition: CachePostcondition::RemoveThenInsertRefreshesPath,
            relations: &[
                RelationVariant::ActiveDependency,
                RelationVariant::MissingDependency,
                RelationVariant::Entry,
            ],
        },
        VfsCacheMatrixRow {
            id: OperationId::O07,
            postcondition: CachePostcondition::InsertRefreshesCurrentSource,
            relations: &[
                RelationVariant::Entry,
                RelationVariant::ActiveDependency,
                RelationVariant::AssetDependency,
            ],
        },
        VfsCacheMatrixRow {
            id: OperationId::O08,
            postcondition: CachePostcondition::RenameRetiresOldPath,
            relations: &[
                RelationVariant::ActiveDependency,
                RelationVariant::UnrelatedPath,
            ],
        },
        VfsCacheMatrixRow {
            id: OperationId::O09,
            postcondition: CachePostcondition::RenameRetiresOldPath,
            relations: &[RelationVariant::ActiveDependency, RelationVariant::Entry],
        },
        VfsCacheMatrixRow {
            id: OperationId::O10,
            postcondition: CachePostcondition::RenameRetiresOldPath,
            relations: &[RelationVariant::Entry, RelationVariant::ActiveDependency],
        },
        VfsCacheMatrixRow {
            id: OperationId::O11,
            postcondition: CachePostcondition::RootBoundaryRetiresOldPath,
            relations: &[
                RelationVariant::ActiveDependency,
                RelationVariant::MissingDependency,
                RelationVariant::UnrelatedPath,
            ],
        },
        VfsCacheMatrixRow {
            id: OperationId::O12,
            postcondition: CachePostcondition::PrefixRewriteRetiresOldPaths,
            relations: &[
                RelationVariant::ActiveDependency,
                RelationVariant::UnrelatedPath,
            ],
        },
        VfsCacheMatrixRow {
            id: OperationId::O13,
            postcondition: CachePostcondition::PrefixRewriteRetiresOldPaths,
            relations: &[RelationVariant::ActiveDependency, RelationVariant::Entry],
        },
        VfsCacheMatrixRow {
            id: OperationId::O14,
            postcondition: CachePostcondition::PrefixRewriteRetiresOldPaths,
            relations: &[
                RelationVariant::ActiveDependency,
                RelationVariant::UnrelatedPath,
                RelationVariant::Entry,
            ],
        },
        VfsCacheMatrixRow {
            id: OperationId::O15,
            postcondition: CachePostcondition::RootBoundaryRetiresOldPath,
            relations: &[
                RelationVariant::ActiveDependency,
                RelationVariant::UnrelatedPath,
            ],
        },
        VfsCacheMatrixRow {
            id: OperationId::O16,
            postcondition: CachePostcondition::NoDirectVfsChange,
            relations: &[RelationVariant::RetainedInactiveDependency],
        },
        VfsCacheMatrixRow {
            id: OperationId::O17,
            postcondition: CachePostcondition::InsertRefreshesCurrentSource,
            relations: &[
                RelationVariant::RetainedInactiveDependency,
                RelationVariant::ActiveDependency,
            ],
        },
        VfsCacheMatrixRow {
            id: OperationId::O18,
            postcondition: CachePostcondition::ShadowOverlayOrdersWithFilesystem,
            relations: &[
                RelationVariant::ShadowOpenPath,
                RelationVariant::Entry,
                RelationVariant::ActiveDependency,
            ],
        },
        VfsCacheMatrixRow {
            id: OperationId::O19,
            postcondition: CachePostcondition::InsertRefreshesCurrentSource,
            relations: &[
                RelationVariant::ActiveDependency,
                RelationVariant::AssetDependency,
                RelationVariant::UnrelatedPath,
            ],
        },
        VfsCacheMatrixRow {
            id: OperationId::O20,
            postcondition: CachePostcondition::MixedBatchFinalState,
            relations: &[
                RelationVariant::Entry,
                RelationVariant::ActiveDependency,
                RelationVariant::UnrelatedPath,
            ],
        },
    ];

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

    #[test]
    fn vfs_cache_file_operation_matrix_is_explicit() {
        for id in [
            OperationId::O01,
            OperationId::O02,
            OperationId::O03,
            OperationId::O04,
            OperationId::O05,
            OperationId::O06,
            OperationId::O07,
            OperationId::O08,
            OperationId::O09,
            OperationId::O10,
            OperationId::O11,
            OperationId::O12,
            OperationId::O13,
            OperationId::O14,
            OperationId::O15,
            OperationId::O16,
            OperationId::O17,
            OperationId::O18,
            OperationId::O19,
            OperationId::O20,
        ] {
            assert_matrix_contains(id, |row| row.id == id);
        }

        for relation in [
            RelationVariant::Entry,
            RelationVariant::ActiveDependency,
            RelationVariant::MissingDependency,
            RelationVariant::RetainedInactiveDependency,
            RelationVariant::AssetDependency,
            RelationVariant::ShadowOpenPath,
            RelationVariant::UnrelatedPath,
        ] {
            assert_matrix_contains(relation, |row| row.relations.contains(&relation));
        }

        for postcondition in [
            CachePostcondition::InsertRefreshesCurrentSource,
            CachePostcondition::SamePathReadErrorReplacesSource,
            CachePostcondition::RemoveRetiresPath,
            CachePostcondition::RemoveThenInsertRefreshesPath,
            CachePostcondition::RenameRetiresOldPath,
            CachePostcondition::PrefixRewriteRetiresOldPaths,
            CachePostcondition::RootBoundaryRetiresOldPath,
            CachePostcondition::NoDirectVfsChange,
            CachePostcondition::ShadowOverlayOrdersWithFilesystem,
            CachePostcondition::MixedBatchFinalState,
        ] {
            assert_matrix_contains(postcondition, |row| row.postcondition == postcondition);
            assert!(
                !postcondition.grouping_note().is_empty(),
                "postcondition {postcondition:?} must document why grouped rows share behavior"
            );
        }
    }

    #[test]
    fn vfs_cache_file_operation_matrix_rows_execute_expected_state() {
        for row in VFS_CACHE_FILE_OPERATION_MATRIX {
            run_vfs_cache_matrix_row(*row);
        }
    }

    fn run_vfs_cache_matrix_row(row: VfsCacheMatrixRow) {
        match row.id {
            OperationId::O01 => assert_create_row(row),
            OperationId::O02 => assert_content_update_row(row),
            OperationId::O03 => assert_transient_empty_row(row),
            OperationId::O04 => assert_read_error_row(row),
            OperationId::O05 => assert_remove_file_row(row),
            OperationId::O06 => assert_delete_then_recreate_row(row),
            OperationId::O07 => assert_atomic_replace_row(row),
            OperationId::O08 => assert_rename_stale_row(row),
            OperationId::O09 => assert_rename_updated_row(row),
            OperationId::O10 => assert_case_only_rename_row(row),
            OperationId::O11 => assert_move_file_root_boundary_row(row),
            OperationId::O12 => assert_rename_directory_stale_row(row),
            OperationId::O13 => assert_rename_directory_updated_row(row),
            OperationId::O14 => assert_delete_directory_row(row),
            OperationId::O15 => assert_move_directory_root_boundary_row(row),
            OperationId::O16 => assert_membership_remove_row(row),
            OperationId::O17 => assert_membership_add_row(row),
            OperationId::O18 => assert_shadow_filesystem_race_row(row),
            OperationId::O19 => assert_symlink_like_observable_change_row(row),
            OperationId::O20 => assert_mixed_batch_row(row),
        }
    }

    fn assert_create_row(row: VfsCacheMatrixRow) {
        assert_eq!(
            row.postcondition,
            CachePostcondition::InsertRefreshesCurrentSource
        );

        let workspace = base_workspace();
        let mut vfs = workspace.vfs();
        let id = workspace.file_id("new.typ").unwrap();
        assert!(vfs.source(id).is_err(), "{} precondition", row.id.label());
        let revision = vfs.revision().get();

        workspace
            .create_source("new.typ", "#let value = [created]")
            .apply_to_vfs(&mut vfs);

        assert_source(
            row.id,
            &vfs,
            &workspace,
            "new.typ",
            "#let value = [created]",
        );
        assert_dirty_since(row.id, &vfs, revision, id, "new.typ");
    }

    fn assert_content_update_row(row: VfsCacheMatrixRow) {
        assert_eq!(
            row.postcondition,
            CachePostcondition::InsertRefreshesCurrentSource
        );

        let workspace = base_workspace();
        let mut vfs = workspace.vfs();
        let dep_id = workspace.file_id(DEP).unwrap();
        let asset_id = workspace.file_id(ASSET).unwrap();
        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [before]");
        assert_eq!(
            vfs.read(asset_id).unwrap(),
            Bytes::from_string("asset-before".to_owned())
        );
        let revision = vfs.revision().get();

        workspace
            .update_source(DEP, "#let value = [after]")
            .apply_to_vfs(&mut vfs);
        workspace
            .write_bytes(ASSET, Bytes::from_string("asset-after".to_owned()))
            .apply_to_vfs(&mut vfs);

        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [after]");
        assert_eq!(
            vfs.read(asset_id).unwrap(),
            Bytes::from_string("asset-after".to_owned()),
            "{} asset bytes should refresh",
            row.id.label()
        );
        assert_dirty_since(row.id, &vfs, revision, dep_id, DEP);
    }

    fn assert_transient_empty_row(row: VfsCacheMatrixRow) {
        assert_eq!(
            row.postcondition,
            CachePostcondition::InsertRefreshesCurrentSource
        );

        let workspace = base_workspace();
        let mut vfs = workspace.vfs();
        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [before]");

        workspace.update_source(DEP, "").apply_to_vfs(&mut vfs);
        assert_source(row.id, &vfs, &workspace, DEP, "");

        workspace
            .update_source(DEP, "#let value = [after empty]")
            .apply_to_vfs(&mut vfs);
        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [after empty]");
    }

    fn assert_read_error_row(row: VfsCacheMatrixRow) {
        assert_eq!(
            row.postcondition,
            CachePostcondition::SamePathReadErrorReplacesSource
        );

        let workspace = base_workspace();
        let mut vfs = workspace.vfs();
        let dep_id = workspace.file_id(DEP).unwrap();
        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [before]");
        let revision = vfs.revision().get();

        read_error_change(&workspace, DEP).apply_to_vfs(&mut vfs);

        assert_source_unavailable(row.id, &vfs, &workspace, DEP);
        assert_dirty_since(row.id, &vfs, revision, dep_id, DEP);

        workspace
            .update_source(DEP, "#let value = [recovered]")
            .apply_to_vfs(&mut vfs);
        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [recovered]");
    }

    fn assert_remove_file_row(row: VfsCacheMatrixRow) {
        assert_eq!(row.postcondition, CachePostcondition::RemoveRetiresPath);

        let workspace = base_workspace();
        let mut vfs = workspace.vfs();
        let dep_id = workspace.file_id(DEP).unwrap();
        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [before]");
        let revision = vfs.revision().get();

        workspace.remove(DEP).unwrap().apply_to_vfs(&mut vfs);

        assert_source_unavailable(row.id, &vfs, &workspace, DEP);
        assert_dirty_since(row.id, &vfs, revision, dep_id, DEP);
    }

    fn assert_delete_then_recreate_row(row: VfsCacheMatrixRow) {
        assert_eq!(
            row.postcondition,
            CachePostcondition::RemoveThenInsertRefreshesPath
        );

        let workspace = base_workspace();
        let mut vfs = workspace.vfs();
        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [before]");

        workspace.remove(DEP).unwrap().apply_to_vfs(&mut vfs);
        assert_source_unavailable(row.id, &vfs, &workspace, DEP);

        workspace
            .create_source(DEP, "#let value = [recreated]")
            .apply_to_vfs(&mut vfs);
        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [recreated]");
    }

    fn assert_atomic_replace_row(row: VfsCacheMatrixRow) {
        assert_eq!(
            row.postcondition,
            CachePostcondition::InsertRefreshesCurrentSource
        );

        let workspace = base_workspace();
        let mut vfs = workspace.vfs();
        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [before]");

        let replace = replace_source_change(&workspace, DEP, "#let value = [replaced]");
        replace.apply_to_vfs(&mut vfs);

        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [replaced]");
    }

    fn assert_rename_stale_row(row: VfsCacheMatrixRow) {
        assert_eq!(row.postcondition, CachePostcondition::RenameRetiresOldPath);

        let workspace = base_workspace();
        let mut vfs = workspace.vfs();
        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [before]");

        workspace
            .rename(DEP, RENAMED_DEP)
            .unwrap()
            .apply_to_vfs(&mut vfs);

        assert_source_unavailable(row.id, &vfs, &workspace, DEP);
        assert_source(
            row.id,
            &vfs,
            &workspace,
            RENAMED_DEP,
            "#let value = [before]",
        );
    }

    fn assert_rename_updated_row(row: VfsCacheMatrixRow) {
        assert_eq!(row.postcondition, CachePostcondition::RenameRetiresOldPath);

        let workspace = base_workspace();
        let mut vfs = workspace.vfs();
        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [before]");

        let rename = workspace.rename(DEP, RENAMED_DEP).unwrap();
        let entry = workspace.update_source(ENTRY, "#import \"renamed.typ\": value\n#value");
        combine_changes(&[rename, entry]).apply_to_vfs(&mut vfs);

        assert_source_unavailable(row.id, &vfs, &workspace, DEP);
        assert_source(
            row.id,
            &vfs,
            &workspace,
            RENAMED_DEP,
            "#let value = [before]",
        );
        assert_source(
            row.id,
            &vfs,
            &workspace,
            ENTRY,
            "#import \"renamed.typ\": value\n#value",
        );
    }

    fn assert_case_only_rename_row(row: VfsCacheMatrixRow) {
        assert_eq!(row.postcondition, CachePostcondition::RenameRetiresOldPath);

        let workspace = MockWorkspace::default_builder()
            .file("case.typ", "#let value = [case]")
            .build();
        let mut vfs = workspace.vfs();
        assert_source(row.id, &vfs, &workspace, "case.typ", "#let value = [case]");

        workspace
            .rename("case.typ", "Case.typ")
            .unwrap()
            .apply_to_vfs(&mut vfs);

        assert_source_unavailable(row.id, &vfs, &workspace, "case.typ");
        assert_source(row.id, &vfs, &workspace, "Case.typ", "#let value = [case]");
    }

    fn assert_move_file_root_boundary_row(row: VfsCacheMatrixRow) {
        assert_eq!(
            row.postcondition,
            CachePostcondition::RootBoundaryRetiresOldPath
        );

        let workspace = base_workspace();
        let mut vfs = workspace.vfs();
        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [before]");

        let removed = workspace.remove(DEP).unwrap();
        let moved_out = workspace.write_source("/outside-root/dep.typ", "#let value = [outside]");
        combine_changes(&[removed, moved_out]).apply_to_vfs(&mut vfs);

        assert_source_unavailable(row.id, &vfs, &workspace, DEP);
    }

    fn assert_rename_directory_stale_row(row: VfsCacheMatrixRow) {
        assert_eq!(
            row.postcondition,
            CachePostcondition::PrefixRewriteRetiresOldPaths
        );

        let workspace = directory_workspace();
        let mut vfs = workspace.vfs();
        assert_source(
            row.id,
            &vfs,
            &workspace,
            "chapters/dep.typ",
            "#let value = [chapter]",
        );

        let remove_dep = workspace.remove("chapters/dep.typ").unwrap();
        let create_dep = workspace.create_source("renamed/dep.typ", "#let value = [chapter]");
        combine_changes(&[remove_dep, create_dep]).apply_to_vfs(&mut vfs);

        assert_source_unavailable(row.id, &vfs, &workspace, "chapters/dep.typ");
        assert_source(
            row.id,
            &vfs,
            &workspace,
            "renamed/dep.typ",
            "#let value = [chapter]",
        );
    }

    fn assert_rename_directory_updated_row(row: VfsCacheMatrixRow) {
        assert_eq!(
            row.postcondition,
            CachePostcondition::PrefixRewriteRetiresOldPaths
        );

        let workspace = directory_workspace();
        let mut vfs = workspace.vfs();
        assert_source(
            row.id,
            &vfs,
            &workspace,
            "chapters/dep.typ",
            "#let value = [chapter]",
        );

        let remove_dep = workspace.remove("chapters/dep.typ").unwrap();
        let create_dep = workspace.create_source("renamed/dep.typ", "#let value = [chapter]");
        let entry = workspace.update_source(ENTRY, "#import \"renamed/dep.typ\": value\n#value");
        combine_changes(&[remove_dep, create_dep, entry]).apply_to_vfs(&mut vfs);

        assert_source_unavailable(row.id, &vfs, &workspace, "chapters/dep.typ");
        assert_source(
            row.id,
            &vfs,
            &workspace,
            "renamed/dep.typ",
            "#let value = [chapter]",
        );
        assert_source(
            row.id,
            &vfs,
            &workspace,
            ENTRY,
            "#import \"renamed/dep.typ\": value\n#value",
        );
    }

    fn assert_delete_directory_row(row: VfsCacheMatrixRow) {
        assert_eq!(
            row.postcondition,
            CachePostcondition::PrefixRewriteRetiresOldPaths
        );

        let workspace = MockWorkspace::default_builder()
            .file(
                ENTRY,
                "#import \"chapters/a.typ\": a\n#import \"chapters/b.typ\": b\n#a\n#b",
            )
            .file("chapters/a.typ", "#let a = [a]")
            .file("chapters/b.typ", "#let b = [b]")
            .file("chapters/note.typ", "#let note = [unused]")
            .build();
        let mut vfs = workspace.vfs();
        assert_source(row.id, &vfs, &workspace, "chapters/a.typ", "#let a = [a]");
        assert_source(row.id, &vfs, &workspace, "chapters/b.typ", "#let b = [b]");

        let remove_a = workspace.remove("chapters/a.typ").unwrap();
        let remove_b = workspace.remove("chapters/b.typ").unwrap();
        let remove_note = workspace.remove("chapters/note.typ").unwrap();
        combine_changes(&[remove_a, remove_b, remove_note]).apply_to_vfs(&mut vfs);

        assert_source_unavailable(row.id, &vfs, &workspace, "chapters/a.typ");
        assert_source_unavailable(row.id, &vfs, &workspace, "chapters/b.typ");
    }

    fn assert_move_directory_root_boundary_row(row: VfsCacheMatrixRow) {
        assert_eq!(
            row.postcondition,
            CachePostcondition::RootBoundaryRetiresOldPath
        );

        let workspace = directory_workspace();
        let mut vfs = workspace.vfs();
        assert_source(
            row.id,
            &vfs,
            &workspace,
            "chapters/dep.typ",
            "#let value = [chapter]",
        );

        let remove_dep = workspace.remove("chapters/dep.typ").unwrap();
        let moved_out =
            workspace.write_source("/outside-root/chapters/dep.typ", "#let value = [chapter]");
        combine_changes(&[remove_dep, moved_out]).apply_to_vfs(&mut vfs);

        assert_source_unavailable(row.id, &vfs, &workspace, "chapters/dep.typ");
    }

    fn assert_membership_remove_row(row: VfsCacheMatrixRow) {
        assert_eq!(row.postcondition, CachePostcondition::NoDirectVfsChange);

        let workspace = base_workspace();
        let mut vfs = workspace.vfs();
        let dep_id = workspace.file_id(DEP).unwrap();
        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [before]");
        let revision = vfs.revision().get();

        empty_change().apply_to_vfs(&mut vfs);

        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [before]");
        assert!(
            vfs.is_clean_compile(revision, &[dep_id]),
            "{} no direct VFS change should keep {DEP:?} clean",
            row.id.label()
        );
    }

    fn assert_membership_add_row(row: VfsCacheMatrixRow) {
        assert_eq!(
            row.postcondition,
            CachePostcondition::InsertRefreshesCurrentSource
        );

        let workspace = base_workspace();
        let mut vfs = workspace.vfs();
        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [before]");

        workspace
            .update_source(DEP, "#let value = [changed while inactive]")
            .apply_to_vfs(&mut vfs);

        assert_source(
            row.id,
            &vfs,
            &workspace,
            DEP,
            "#let value = [changed while inactive]",
        );
    }

    fn assert_shadow_filesystem_race_row(row: VfsCacheMatrixRow) {
        assert_eq!(
            row.postcondition,
            CachePostcondition::ShadowOverlayOrdersWithFilesystem
        );

        let workspace = base_workspace();
        let mut vfs = workspace.vfs();
        let entry_path = workspace.path(ENTRY);
        assert_source(
            row.id,
            &vfs,
            &workspace,
            ENTRY,
            "#import \"dep.typ\": value\n#value",
        );

        vfs.revise()
            .map_shadow(
                &entry_path,
                snapshot(Bytes::from_string(
                    "#let value = [memory]\n#value".to_owned(),
                )),
            )
            .unwrap();
        assert_source(
            row.id,
            &vfs,
            &workspace,
            ENTRY,
            "#let value = [memory]\n#value",
        );

        workspace
            .update_source(ENTRY, "#let value = [filesystem]\n#value")
            .apply_to_vfs(&mut vfs);
        assert_source(
            row.id,
            &vfs,
            &workspace,
            ENTRY,
            "#let value = [memory]\n#value",
        );

        vfs.revise().unmap_shadow(&entry_path).unwrap();
        assert_source(
            row.id,
            &vfs,
            &workspace,
            ENTRY,
            "#let value = [filesystem]\n#value",
        );
    }

    fn assert_symlink_like_observable_change_row(row: VfsCacheMatrixRow) {
        assert_eq!(
            row.postcondition,
            CachePostcondition::InsertRefreshesCurrentSource
        );

        let workspace = MockWorkspace::default_builder()
            .file("linked.typ", "#let value = [target-a]")
            .build();
        let mut vfs = workspace.vfs();
        assert_source(
            row.id,
            &vfs,
            &workspace,
            "linked.typ",
            "#let value = [target-a]",
        );

        workspace
            .update_source("linked.typ", "#let value = [target-b]")
            .apply_to_vfs(&mut vfs);

        assert_source(
            row.id,
            &vfs,
            &workspace,
            "linked.typ",
            "#let value = [target-b]",
        );
    }

    fn assert_mixed_batch_row(row: VfsCacheMatrixRow) {
        assert_eq!(row.postcondition, CachePostcondition::MixedBatchFinalState);

        let workspace = base_workspace();
        let mut vfs = workspace.vfs();
        assert_source(
            row.id,
            &vfs,
            &workspace,
            ENTRY,
            "#import \"dep.typ\": value\n#value",
        );
        assert_source(row.id, &vfs, &workspace, DEP, "#let value = [before]");

        let rename = workspace.rename(DEP, RENAMED_DEP).unwrap();
        let entry = workspace.update_source(ENTRY, "#import \"renamed.typ\": value\n#value");
        let unrelated = workspace.update_source(UNRELATED, "#let note = [changed]");
        let created = workspace.create_source("created.typ", "#let created = [created]");
        combine_changes(&[rename, entry, unrelated, created]).apply_to_vfs(&mut vfs);

        assert_source_unavailable(row.id, &vfs, &workspace, DEP);
        assert_source(
            row.id,
            &vfs,
            &workspace,
            RENAMED_DEP,
            "#let value = [before]",
        );
        assert_source(
            row.id,
            &vfs,
            &workspace,
            ENTRY,
            "#import \"renamed.typ\": value\n#value",
        );
        assert_source(row.id, &vfs, &workspace, UNRELATED, "#let note = [changed]");
        assert_source(
            row.id,
            &vfs,
            &workspace,
            "created.typ",
            "#let created = [created]",
        );
    }

    fn base_workspace() -> MockWorkspace {
        MockWorkspace::default_builder()
            .file(ENTRY, "#import \"dep.typ\": value\n#value")
            .file(DEP, "#let value = [before]")
            .file(UNRELATED, "#let note = [unchanged]")
            .bytes(ASSET, Bytes::from_string("asset-before".to_owned()))
            .build()
    }

    fn directory_workspace() -> MockWorkspace {
        MockWorkspace::default_builder()
            .file(ENTRY, "#import \"chapters/dep.typ\": value\n#value")
            .file("chapters/dep.typ", "#let value = [chapter]")
            .file("chapters/unrelated.typ", "#let note = [unused]")
            .build()
    }

    fn read_error_change(workspace: &MockWorkspace, path: &str) -> MockChange {
        let snapshot = FileResult::Err(FileError::NotFound(workspace.path(path))).into();
        MockChange::new(FileChangeSet::new_inserts(vec![(
            workspace.immut_path(path),
            snapshot,
        )]))
    }

    fn replace_source_change(workspace: &MockWorkspace, path: &str, source: &str) -> MockChange {
        let removed = workspace.remove(path).unwrap();
        let created = workspace.create_source(path, source);
        combine_changes(&[removed, created])
    }

    fn empty_change() -> MockChange {
        MockChange::new(FileChangeSet::default())
    }

    fn combine_changes(changes: &[MockChange]) -> MockChange {
        let mut changeset = FileChangeSet::default();
        for change in changes {
            changeset.removes.extend(change.changeset().removes.clone());
            changeset.inserts.extend(change.changeset().inserts.clone());
        }

        MockChange::new(changeset)
    }

    fn assert_matrix_contains<T: std::fmt::Debug>(
        missing: T,
        predicate: impl Fn(&VfsCacheMatrixRow) -> bool,
    ) {
        assert!(
            VFS_CACHE_FILE_OPERATION_MATRIX.iter().any(predicate),
            "VFS/cache file-operation matrix missing {missing:?}"
        );
    }

    fn assert_source(
        id: OperationId,
        vfs: &Vfs<MockPathAccess>,
        workspace: &MockWorkspace,
        path: &str,
        expected: &str,
    ) {
        let file_id = workspace.file_id(path).unwrap_or_else(|err| {
            panic!(
                "{} failed to resolve file id for {path:?}: {err:?}",
                id.label()
            )
        });
        let source = vfs.source(file_id).unwrap_or_else(|err| {
            panic!(
                "{} expected source for {path:?}, got error: {err:?}",
                id.label()
            )
        });
        assert_eq!(
            source.text(),
            expected,
            "{} source mismatch for {path:?}",
            id.label()
        );
    }

    fn assert_source_unavailable(
        id: OperationId,
        vfs: &Vfs<MockPathAccess>,
        workspace: &MockWorkspace,
        path: &str,
    ) {
        let file_id = workspace.file_id(path).unwrap_or_else(|err| {
            panic!(
                "{} failed to resolve file id for {path:?}: {err:?}",
                id.label()
            )
        });
        if let Ok(source) = vfs.source(file_id) {
            panic!(
                "{} expected {path:?} to be unavailable, got {:?}",
                id.label(),
                source.text()
            );
        }
    }

    fn assert_dirty_since(
        id: OperationId,
        vfs: &Vfs<MockPathAccess>,
        revision: usize,
        file_id: FileId,
        path: &str,
    ) {
        assert!(
            !vfs.is_clean_compile(revision, &[file_id]),
            "{} expected {path:?} to be dirty since revision {revision}",
            id.label()
        );
    }
}
