use std::hash::Hash;
use std::path::PathBuf;

use ecow::EcoVec;
use tinymist_std::error::prelude::*;
use tinymist_std::{bail, ImmutPath};
use typst::diag::EcoString;

pub use task::*;
pub use tinymist_task as task;

/// The currently using lock file version.
pub const LOCK_VERSION: &str = "0.1.0-beta0";

/// A lock file compatibility wrapper.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case", tag = "version")]
pub enum LockFileCompat {
    /// The lock file schema with version 0.1.0-beta0.
    #[serde(rename = "0.1.0-beta0")]
    Version010Beta0(LockFile),
    /// Other lock file schema.
    #[serde(untagged)]
    Other(serde_json::Value),
}

impl LockFileCompat {
    /// Returns the lock file version.
    pub fn version(&self) -> Result<&str> {
        match self {
            LockFileCompat::Version010Beta0(..) => Ok(LOCK_VERSION),
            LockFileCompat::Other(v) => v
                .get("version")
                .and_then(|v| v.as_str())
                .context("missing version field"),
        }
    }

    /// Migrates the lock file to the current version.
    pub fn migrate(self) -> Result<LockFile> {
        match self {
            LockFileCompat::Version010Beta0(v) => Ok(v),
            this @ LockFileCompat::Other(..) => {
                bail!(
                    "cannot migrate from version: {}",
                    this.version().unwrap_or("unknown version")
                )
            }
        }
    }
}

/// A lock file storing project information.
#[derive(Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct LockFile {
    // The lock file version.
    // version: String,
    /// The project's document (input).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub document: Vec<ProjectInput>,
    /// The project's task (output).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub task: Vec<ApplyProjectTask>,
    /// The project's task route.
    #[serde(skip_serializing_if = "EcoVec::is_empty", default)]
    pub route: EcoVec<ProjectRoute>,
}

/// A project input specifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectInput {
    /// The project's ID.
    pub id: Id,
    /// The path to the root directory of the project.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<ResourcePath>,
    /// The path to the main file of the project.
    pub main: ResourcePath,
    /// The key-value pairs visible through `sys.inputs`
    pub inputs: Vec<(String, String)>,
    /// The project's font paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub font_paths: Vec<ResourcePath>,
    /// Whether to use system fonts.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub system_fonts: bool,
    /// The project's package path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_path: Option<ResourcePath>,
    /// The project's package cache path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_cache_path: Option<ResourcePath>,
}

/// A project route specifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectMaterial {
    /// The root of the project that the material belongs to.
    pub root: EcoString,
    /// A project.
    pub id: Id,
    /// The files.
    pub files: Vec<ResourcePath>,
}

/// A project route specifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectPathMaterial {
    /// The root of the project that the material belongs to.
    pub root: EcoString,
    /// A project.
    pub id: Id,
    /// The files.
    pub files: Vec<PathBuf>,
}

impl ProjectPathMaterial {
    /// Creates a new project path material from a document ID and a list of
    /// files.
    pub fn from_deps(doc_id: Id, files: EcoVec<ImmutPath>) -> Self {
        let mut files: Vec<_> = files.into_iter().map(|p| p.as_ref().to_owned()).collect();
        files.sort();

        ProjectPathMaterial {
            root: EcoString::default(),
            id: doc_id,
            files,
        }
    }
}

/// A project route specifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectRoute {
    /// A project.
    pub id: Id,
    /// The priority of the project. (lower numbers are higher priority).
    pub priority: u32,
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tinymist_task::PathPattern;
    use tinymist_world::EntryState;
    use typst::syntax::VirtualPath;

    use super::*;

    #[test]
    fn test_substitute_path() {
        let root = Path::new("/root");
        let entry =
            EntryState::new_rooted(root.into(), Some(VirtualPath::new("/dir1/dir2/file.txt")));

        assert_eq!(
            PathPattern::new("/substitute/$dir/$name").substitute(&entry),
            Some(PathBuf::from("/substitute/dir1/dir2/file.txt").into())
        );
        assert_eq!(
            PathPattern::new("/substitute/$dir/../$name").substitute(&entry),
            Some(PathBuf::from("/substitute/dir1/file.txt").into())
        );
        assert_eq!(
            PathPattern::new("/substitute/$name").substitute(&entry),
            Some(PathBuf::from("/substitute/file.txt").into())
        );
        assert_eq!(
            PathPattern::new("/substitute/target/$dir/$name").substitute(&entry),
            Some(PathBuf::from("/substitute/target/dir1/dir2/file.txt").into())
        );
    }
}
