use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use serde::{Deserialize, Serialize};
use tinymist_std::{error::prelude::*, ImmutPath};
use tinymist_vfs::{WorkspaceResolution, WorkspaceResolver};
use typst::diag::SourceResult;
use typst::syntax::{FileId, VirtualPath};

pub trait EntryReader {
    fn entry_state(&self) -> EntryState;

    fn workspace_root(&self) -> Option<Arc<Path>> {
        self.entry_state().root().clone()
    }

    fn main_id(&self) -> Option<FileId> {
        self.entry_state().main()
    }
}

pub trait EntryManager: EntryReader {
    fn reset(&mut self) -> SourceResult<()> {
        Ok(())
    }

    fn mutate_entry(&mut self, state: EntryState) -> SourceResult<EntryState>;
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Default)]
pub struct EntryState {
    /// Path to the root directory of compilation.
    /// The world forbids direct access to files outside this directory.
    root: Option<ImmutPath>,
    /// Identifier of the main file in the workspace
    main: Option<FileId>,
}

pub static DETACHED_ENTRY: LazyLock<FileId> =
    LazyLock::new(|| FileId::new(None, VirtualPath::new(Path::new("/__detached.typ"))));

pub static MEMORY_MAIN_ENTRY: LazyLock<FileId> =
    LazyLock::new(|| FileId::new(None, VirtualPath::new(Path::new("/__main__.typ"))));

impl EntryState {
    /// Create an entry state with no workspace root and no main file.
    pub fn new_detached() -> Self {
        Self {
            root: None,
            main: None,
        }
    }

    /// Create an entry state with a workspace root and no main file.
    pub fn new_workspace(root: ImmutPath) -> Self {
        Self::new_rooted(root, None)
    }

    /// Create an entry state with a workspace root and an optional main file.
    pub fn new_rooted(root: ImmutPath, main: Option<VirtualPath>) -> Self {
        let main = main.map(|main| WorkspaceResolver::workspace_file(Some(&root), main));
        Self {
            root: Some(root),
            main,
        }
    }

    /// Create an entry state with only a main file given.
    pub fn new_rooted_by_parent(entry: ImmutPath) -> Option<Self> {
        let root = entry.parent().map(ImmutPath::from);
        let main =
            WorkspaceResolver::workspace_file(root.as_ref(), VirtualPath::new(entry.file_name()?));

        Some(Self {
            root,
            main: Some(main),
        })
    }

    pub fn main(&self) -> Option<FileId> {
        self.main
    }

    pub fn root(&self) -> Option<ImmutPath> {
        self.root.clone()
    }

    pub fn workspace_root(&self) -> Option<ImmutPath> {
        if let Some(main) = self.main {
            let pkg = WorkspaceResolver::resolve(main).ok()?;
            match pkg {
                WorkspaceResolution::Workspace(id) => Some(id.path().clone()),
                WorkspaceResolution::Package => self.root.clone(),
            }
        } else {
            self.root.clone()
        }
    }

    pub fn select_in_workspace(&self, id: &Path) -> EntryState {
        let id = WorkspaceResolver::workspace_file(self.root.as_ref(), VirtualPath::new(id));

        Self {
            root: self.root.clone(),
            main: Some(id),
        }
    }

    pub fn try_select_path_in_workspace(&self, p: &Path) -> ZResult<Option<EntryState>> {
        Ok(match self.workspace_root() {
            Some(root) => match p.strip_prefix(&root) {
                Ok(p) => Some(EntryState::new_rooted(
                    root.clone(),
                    Some(VirtualPath::new(p)),
                )),
                Err(e) => {
                    return Err(
                        error_once!("entry file is not in workspace", err: e, entry: p.display(), root: root.display()),
                    )
                }
            },
            None => EntryState::new_rooted_by_parent(p.into()),
        })
    }

    pub fn is_detached(&self) -> bool {
        self.root.is_none() && self.main.is_none()
    }

    pub fn is_inactive(&self) -> bool {
        self.main.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EntryOpts {
    Workspace {
        /// Path to the root directory of compilation.
        /// The world forbids direct access to files outside this directory.
        root: PathBuf,
        /// Relative path to the main file in the workspace.
        entry: Option<PathBuf>,
    },
    RootByParent {
        /// Path to the entry file of compilation.
        entry: PathBuf,
    },
    Detached,
}

impl Default for EntryOpts {
    fn default() -> Self {
        Self::Detached
    }
}

impl EntryOpts {
    pub fn new_detached() -> Self {
        Self::Detached
    }

    pub fn new_workspace(root: PathBuf) -> Self {
        Self::Workspace { root, entry: None }
    }

    pub fn new_rooted(root: PathBuf, entry: Option<PathBuf>) -> Self {
        Self::Workspace { root, entry }
    }

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
            EntryOpts::Workspace { root, entry } => Ok(EntryState::new_rooted(
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
