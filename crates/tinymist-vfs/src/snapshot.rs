use core::fmt;

use typst::diag::{FileError, FileResult};

use crate::{Bytes, ImmutPath};

/// A file snapshot that is notified by some external source
///
/// Note: The error is boxed to avoid large stack size
#[derive(Clone, PartialEq, Eq)]
pub struct FileSnapshot(Result<Bytes, Box<FileError>>);

#[derive(Debug)]
#[allow(dead_code)]
struct FileContent {
    len: usize,
}

impl fmt::Debug for FileSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.as_ref() {
            Ok(v) => f
                .debug_struct("FileSnapshot")
                .field("content", &FileContent { len: v.len() })
                .finish(),
            Err(e) => f.debug_struct("FileSnapshot").field("error", &e).finish(),
        }
    }
}

impl FileSnapshot {
    /// content of the file
    #[inline]
    #[track_caller]
    pub fn content(&self) -> FileResult<&Bytes> {
        self.0.as_ref().map_err(|e| *e.clone())
    }

    /// Whether the related file is a file
    #[inline]
    #[track_caller]
    pub fn is_file(&self) -> FileResult<bool> {
        self.content().map(|_| true)
    }
}

impl std::ops::Deref for FileSnapshot {
    type Target = Result<Bytes, Box<FileError>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for FileSnapshot {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Convenient function to create a [`FileSnapshot`] from tuple
impl From<FileResult<Bytes>> for FileSnapshot {
    fn from(result: FileResult<Bytes>) -> Self {
        Self(result.map_err(Box::new))
    }
}

/// A set of changes to the filesystem.
///
/// The correct order of applying changes is:
/// 1. Remove files
/// 2. Upsert (Insert or Update) files
#[derive(Debug, Clone, Default)]
pub struct FileChangeSet {
    /// Files to remove
    pub removes: Vec<ImmutPath>,
    /// Files to insert or update
    pub inserts: Vec<(ImmutPath, FileSnapshot)>,
}

impl FileChangeSet {
    /// Create a new empty changeset
    pub fn is_empty(&self) -> bool {
        self.inserts.is_empty() && self.removes.is_empty()
    }

    /// Create a new changeset with removing files
    pub fn new_removes(removes: Vec<ImmutPath>) -> Self {
        Self {
            removes,
            inserts: vec![],
        }
    }

    /// Create a new changeset with inserting files
    pub fn new_inserts(inserts: Vec<(ImmutPath, FileSnapshot)>) -> Self {
        Self {
            removes: vec![],
            inserts,
        }
    }

    /// Utility function to insert a possible file to insert or update
    pub fn may_insert(&mut self, v: Option<(ImmutPath, FileSnapshot)>) {
        if let Some(v) = v {
            self.inserts.push(v);
        }
    }

    /// Utility function to insert multiple possible files to insert or update
    pub fn may_extend(&mut self, v: Option<impl Iterator<Item = (ImmutPath, FileSnapshot)>>) {
        if let Some(v) = v {
            self.inserts.extend(v);
        }
    }
}
