//! The entry state of the world.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use tinymist_std::{ImmutPath, error::prelude::*};
use tinymist_vfs::{WorkspaceResolution, WorkspaceResolver};
use typst::diag::SourceResult;
use typst::syntax::{FileId, VirtualPath};

/// A trait to read the entry state.
pub trait EntryReader {
    /// Gets the entry state.
    fn entry_state(&self) -> EntryState;

    /// Gets the main file id.
    fn main_id(&self) -> Option<FileId> {
        self.entry_state().main()
    }
}

/// A trait to manage the entry state.
pub trait EntryManager: EntryReader {
    /// Mutates the entry state.
    fn mutate_entry(&mut self, state: EntryState) -> SourceResult<EntryState>;
}

/// The state of the entry.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Default)]
pub struct EntryState {
    /// The path to the root directory of compilation.
    /// The world forbids direct access to files outside this directory.
    ///
    /// If the root is `None`, the world cannot access the file system.
    root: Option<ImmutPath>,
    /// The identifier of the main file in the workspace.
    ///
    /// If the main is `None`, the world is inactive.
    main: Option<FileId>,
}

/// The detached entry.
pub static DETACHED_ENTRY: LazyLock<FileId> =
    LazyLock::new(|| FileId::new(None, VirtualPath::new(Path::new("/__detached.typ"))));

/// The memory main entry.
pub static MEMORY_MAIN_ENTRY: LazyLock<FileId> =
    LazyLock::new(|| FileId::new(None, VirtualPath::new(Path::new("/__main__.typ"))));

impl EntryState {
    /// Creates an entry state with no workspace root and no main file.
    pub fn new_detached() -> Self {
        Self {
            root: None,
            main: None,
        }
    }

    /// Creates an entry state with a workspace root and no main file.
    pub fn new_workspace(root: ImmutPath) -> Self {
        Self::new_rooted(root, None)
    }

    /// Creates an entry state without permission to access the file system.
    pub fn new_rootless(main: VirtualPath) -> Self {
        Self {
            root: None,
            main: Some(FileId::new(None, main)),
        }
    }

    /// Creates an entry state with a workspace root and an main file.
    pub fn new_rooted_by_id(root: ImmutPath, main: FileId) -> Self {
        Self::new_rooted(root, Some(main.vpath().clone()))
    }

    /// Creates an entry state with a workspace root and an optional main file.
    pub fn new_rooted(root: ImmutPath, main: Option<VirtualPath>) -> Self {
        let main = main.map(|main| WorkspaceResolver::workspace_file(Some(&root), main));
        Self {
            root: Some(root),
            main,
        }
    }

    /// Creates an entry state with only a main file given.
    pub fn new_rooted_by_parent(entry: ImmutPath) -> Option<Self> {
        let root = entry.parent().map(ImmutPath::from);
        let main =
            WorkspaceResolver::workspace_file(root.as_ref(), VirtualPath::new(entry.file_name()?));

        Some(Self {
            root,
            main: Some(main),
        })
    }

    /// Gets the main file id.
    pub fn main(&self) -> Option<FileId> {
        self.main
    }

    /// Gets the specified root directory.
    pub fn root(&self) -> Option<ImmutPath> {
        self.root.clone()
    }

    /// Gets the root directory of the main file.
    pub fn workspace_root(&self) -> Option<ImmutPath> {
        if let Some(main) = self.main {
            match WorkspaceResolver::resolve(main).ok()? {
                WorkspaceResolution::Workspace(id) | WorkspaceResolution::UntitledRooted(id) => {
                    Some(id.path().clone())
                }
                WorkspaceResolution::Rootless => None,
                WorkspaceResolution::Package => self.root.clone(),
            }
        } else {
            self.root.clone()
        }
    }

    /// Selects an entry in the workspace.
    pub fn select_in_workspace(&self, path: &Path) -> EntryState {
        let id = WorkspaceResolver::workspace_file(self.root.as_ref(), VirtualPath::new(path));

        Self {
            root: self.root.clone(),
            main: Some(id),
        }
    }

    /// Tries to select an entry in the workspace.
    pub fn try_select_path_in_workspace(&self, path: &Path) -> Result<Option<EntryState>> {
        Ok(match self.workspace_root() {
            Some(root) => match path.strip_prefix(&root) {
                Ok(path) => Some(EntryState::new_rooted(
                    root.clone(),
                    Some(VirtualPath::new(path)),
                )),
                Err(err) => {
                    return Err(
                        error_once!("entry file is not in workspace", err: err, entry: path.display(), root: root.display()),
                    );
                }
            },
            None => EntryState::new_rooted_by_parent(path.into()),
        })
    }

    /// Checks if the world is detached.
    pub fn is_detached(&self) -> bool {
        self.root.is_none() && self.main.is_none()
    }

    /// Checks if the world is inactive.
    pub fn is_inactive(&self) -> bool {
        self.main.is_none()
    }

    /// Checks if the world is in a package.
    pub fn is_in_package(&self) -> bool {
        self.main.is_some_and(WorkspaceResolver::is_package_file)
    }
}

/// The options to create the entry
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum EntryOpts {
    /// Creates the entry with a specified root directory and a main file.
    Workspace {
        /// Path to the root directory of compilation.
        /// The world forbids direct access to files outside this directory.
        root: PathBuf,
        /// Relative path to the main file in the workspace.
        main: Option<PathBuf>,
    },
    /// Creates the entry with a main file and a parent directory as the root.
    RootByParent {
        /// Path to the entry file of compilation.
        entry: PathBuf,
    },
    /// Creates the entry with no root and no main file.
    #[default]
    Detached,
}

impl EntryOpts {
    /// Creates the entry with no root and no main file.
    pub fn new_detached() -> Self {
        Self::Detached
    }

    /// Creates the entry with a specified root directory and no main file.
    pub fn new_workspace(root: PathBuf) -> Self {
        Self::Workspace { root, main: None }
    }

    /// Creates the entry with a specified root directory and a main file.
    pub fn new_rooted(root: PathBuf, main: Option<PathBuf>) -> Self {
        Self::Workspace { root, main }
    }

    /// Creates the entry with a main file and a parent directory as the root.
    pub fn new_rootless(entry: PathBuf) -> Option<Self> {
        if entry.is_relative() {
            return None;
        }

        Some(Self::RootByParent {
            entry: entry.clone(),
        })
    }
}

impl TryFrom<EntryOpts> for EntryState {
    type Error = tinymist_std::Error;

    fn try_from(value: EntryOpts) -> Result<Self, Self::Error> {
        match value {
            EntryOpts::Workspace { root, main: entry } => Ok(EntryState::new_rooted(
                root.as_path().into(),
                entry.map(VirtualPath::new),
            )),
            EntryOpts::RootByParent { entry } => {
                if entry.is_relative() {
                    return Err(error_once!("entry path must be absolute", path: entry.display()));
                }

                // todo: is there path that has no parent?
                EntryState::new_rooted_by_parent(entry.as_path().into())
                    .ok_or_else(|| error_once!("entry path is invalid", path: entry.display()))
            }
            EntryOpts::Detached => Ok(EntryState::new_detached()),
        }
    }
}
