use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use reflexo::{error::prelude::*, ImmutPath};
use serde::{Deserialize, Serialize};
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
    /// The differences is that: if the entry is rooted, the workspace root is
    /// the parent of the entry file and cannot be used by workspace functions
    /// like [`EntryState::try_select_path_in_workspace`].
    rooted: bool,
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
            rooted: false,
            root: None,
            main: None,
        }
    }

    /// Create an entry state with a workspace root and no main file.
    pub fn new_workspace(root: ImmutPath) -> Self {
        Self::new_rooted(root, None)
    }

    /// Create an entry state with a workspace root and an optional main file.
    pub fn new_rooted(root: ImmutPath, main: Option<FileId>) -> Self {
        Self {
            rooted: true,
            root: Some(root),
            main,
        }
    }

    /// Create an entry state with only a main file given.
    pub fn new_rootless(entry: ImmutPath) -> Option<Self> {
        Some(Self {
            rooted: false,
            root: entry.parent().map(From::from),
            main: Some(FileId::new(None, VirtualPath::new(entry.file_name()?))),
        })
    }

    pub fn main(&self) -> Option<FileId> {
        self.main
    }

    pub fn root(&self) -> Option<ImmutPath> {
        self.root.clone()
    }

    pub fn workspace_root(&self) -> Option<ImmutPath> {
        self.rooted.then(|| self.root.clone()).flatten()
    }

    pub fn select_in_workspace(&self, id: FileId) -> EntryState {
        Self {
            rooted: self.rooted,
            root: self.root.clone(),
            main: Some(id),
        }
    }

    pub fn try_select_path_in_workspace(
        &self,
        p: &Path,
        allow_rootless: bool,
    ) -> ZResult<Option<EntryState>> {
        Ok(match self.workspace_root() {
            Some(root) => match p.strip_prefix(&root) {
                Ok(p) => Some(EntryState::new_rooted(
                    root.clone(),
                    Some(FileId::new(None, VirtualPath::new(p))),
                )),
                Err(e) => {
                    return Err(
                        error_once!("entry file is not in workspace", err: e, entry: p.display(), root: root.display()),
                    )
                }
            },
            None if allow_rootless => EntryState::new_rootless(p.into()),
            None => None,
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
    RootlessEntry {
        /// Path to the entry file of compilation.
        entry: PathBuf,
        /// Parent directory of the entry file.
        root: Option<PathBuf>,
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

        Some(Self::RootlessEntry {
            entry: entry.clone(),
            root: entry.parent().map(From::from),
        })
    }
}

impl TryFrom<EntryOpts> for EntryState {
    type Error = reflexo::Error;

    fn try_from(value: EntryOpts) -> Result<Self, Self::Error> {
        match value {
            EntryOpts::Workspace { root, entry } => Ok(EntryState::new_rooted(
                root.as_path().into(),
                entry.map(|e| FileId::new(None, VirtualPath::new(e))),
            )),
            EntryOpts::RootlessEntry { entry, root } => {
                if entry.is_relative() {
                    return Err(error_once!("entry path must be absolute", path: entry.display()));
                }

                // todo: is there path that has no parent?
                let root = root
                    .as_deref()
                    .or_else(|| entry.parent())
                    .ok_or_else(|| error_once!("a root must be determined for EntryOpts::PreparedEntry", path: entry.display()))?;

                let relative_entry = match entry.strip_prefix(root) {
                    Ok(e) => e,
                    Err(_) => {
                        return Err(
                            error_once!("entry path must be inside the root", path: entry.display()),
                        )
                    }
                };

                Ok(EntryState {
                    rooted: false,
                    root: Some(root.into()),
                    main: Some(FileId::new(None, VirtualPath::new(relative_entry))),
                })
            }
            EntryOpts::Detached => Ok(EntryState::new_detached()),
        }
    }
}
