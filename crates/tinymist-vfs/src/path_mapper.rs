//! Maps paths to compact integer ids. We don't care about clearings paths which
//! no longer exist -- the assumption is total size of paths we ever look at is
//! not too big.

use core::fmt;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use parking_lot::RwLock;
use tinymist_std::ImmutPath;
use tinymist_std::path::{PathClean, unix_slash, looks_like_uri};
use typst::diag::{EcoString, FileError, FileResult, eco_format};
use typst::syntax::VirtualPath;
use typst::syntax::package::{PackageSpec, PackageVersion};

use super::FileId;

/// Represents the resolution of a path to either a physical filesystem path or a virtual path.
#[derive(Debug)]
pub enum PathResolution {
    /// A path that has been resolved to a physical filesystem path.
    Resolved(PathBuf),
    /// A path that exists without a physical root, represented as a virtual path.
    Rootless(Cow<'static, VirtualPath>),
}

impl PathResolution {
    /// Converts the path resolution to a file result, returning an error for rootless paths.
    pub fn to_err(self) -> FileResult<PathBuf> {
        match self {
            PathResolution::Resolved(path) => Ok(path),
            PathResolution::Rootless(_) => Err(FileError::AccessDenied),
        }
    }

    /// Returns a reference to the path as a `Path`.
    pub fn as_path(&self) -> &Path {
        match self {
            PathResolution::Resolved(path) => path.as_path(),
            PathResolution::Rootless(path) => path.as_rooted_path(),
        }
    }

    /// Joins the current path with a relative path string.
    pub fn join(&self, path: &str) -> FileResult<PathResolution> {
        match self {
            PathResolution::Resolved(root) => Ok(PathResolution::Resolved(root.join(path))),
            PathResolution::Rootless(root) => {
                Ok(PathResolution::Rootless(Cow::Owned(root.join(path))))
            }
        }
    }

    /// Resolves a virtual path relative to this path resolution.
    pub fn resolve_to(&self, path: &VirtualPath) -> Option<PathResolution> {
        match self {
            PathResolution::Resolved(root) => Some(PathResolution::Resolved(path.resolve(root)?)),
            PathResolution::Rootless(root) => Some(PathResolution::Rootless(Cow::Owned(
                VirtualPath::new(path.resolve(root.as_ref().as_rooted_path())?),
            ))),
        }
    }
}

/// Trait for resolving file paths and roots for different types of files.
pub trait RootResolver {
    /// Resolves a file ID to its corresponding path resolution.
    fn path_for_id(&self, file_id: FileId) -> FileResult<PathResolution> {
        use WorkspaceResolution::*;
        let root = match WorkspaceResolver::resolve(file_id)? {
            Workspace(id) => id.path().clone(),
            Package => {
                self.resolve_package_root(file_id.package().expect("not a file in package"))?
            }
            UntitledRooted(..) | Rootless => {
                return Ok(PathResolution::Rootless(Cow::Borrowed(file_id.vpath())));
            }
        };

        file_id
            .vpath()
            .resolve(&root)
            .map(PathResolution::Resolved)
            .ok_or_else(|| FileError::AccessDenied)
    }

    /// Resolves the root path for a given file ID.
    fn resolve_root(&self, file_id: FileId) -> FileResult<Option<ImmutPath>> {
        use WorkspaceResolution::*;
        match WorkspaceResolver::resolve(file_id)? {
            Workspace(id) | UntitledRooted(id) => Ok(Some(id.path().clone())),
            Rootless => Ok(None),
            Package => self
                .resolve_package_root(file_id.package().expect("not a file in package"))
                .map(Some),
        }
    }

    /// Resolves the root path for a given package specification.
    fn resolve_package_root(&self, pkg: &PackageSpec) -> FileResult<ImmutPath>;
}

/// A unique identifier for a workspace, represented as a 16-bit integer.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct WorkspaceId(u16);

const NO_VERSION: PackageVersion = PackageVersion {
    major: 0,
    minor: 0,
    patch: 0,
};

const UNTITLED_ROOT: PackageVersion = PackageVersion {
    major: 0,
    minor: 0,
    patch: 1,
};

impl WorkspaceId {
    fn package(&self) -> PackageSpec {
        PackageSpec {
            namespace: WorkspaceResolver::WORKSPACE_NS.clone(),
            name: eco_format!("p{}", self.0),
            version: NO_VERSION,
        }
    }

    fn untitled_root(&self) -> PackageSpec {
        PackageSpec {
            namespace: WorkspaceResolver::WORKSPACE_NS.clone(),
            name: eco_format!("p{}", self.0),
            version: UNTITLED_ROOT,
        }
    }

    /// Returns the filesystem path associated with this workspace ID.
    pub fn path(&self) -> ImmutPath {
        let interner = INTERNER.read();
        interner
            .from_id
            .get(self.0 as usize)
            .expect("invalid workspace id")
            .clone()
    }

    fn from_package_name(name: &str) -> Option<WorkspaceId> {
        if !name.starts_with("p") {
            return None;
        }

        let num = name[1..].parse().ok()?;
        Some(WorkspaceId(num))
    }
}


/// The global package-path interner.
static INTERNER: LazyLock<RwLock<Interner>> = LazyLock::new(|| {
    RwLock::new(Interner {
        to_id: HashMap::new(),
        from_id: Vec::new(),
    })
});

/// Represents the different types of workspace resolution for a file.
pub enum WorkspaceResolution {
    /// A file that belongs to a workspace with a specific workspace ID.
    Workspace(WorkspaceId),
    /// A file that is rooted in a workspace but untitled.
    UntitledRooted(WorkspaceId),
    /// A file that has no root and exists without workspace context.
    Rootless,
    /// A file that belongs to a package.
    Package,
}

/// A package-path interner.
struct Interner {
    to_id: HashMap<ImmutPath, WorkspaceId>,
    from_id: Vec<ImmutPath>,
}

/// Resolver for handling workspace-related path operations and file ID management.
#[derive(Default)]
pub struct WorkspaceResolver {}

impl WorkspaceResolver {
    /// Namespace identifier for workspace files.
    pub const WORKSPACE_NS: EcoString = EcoString::inline("ws");

    /// Checks if a file ID represents a workspace file.
    pub fn is_workspace_file(fid: FileId) -> bool {
        fid.package()
            .is_some_and(|p| p.namespace == WorkspaceResolver::WORKSPACE_NS)
    }

    /// Checks if a file ID represents a package file.
    pub fn is_package_file(fid: FileId) -> bool {
        fid.package()
            .is_some_and(|p| p.namespace != WorkspaceResolver::WORKSPACE_NS)
    }

    /// Gets or creates a workspace ID for the given root path.
    pub fn workspace_id(root: &ImmutPath) -> WorkspaceId {
        let root: ImmutPath = {
            let as_str = unix_slash(&*root);
            if looks_like_uri(&as_str) { // avoid running URI roots through `PathClean` because they might be misinterpreted as drive letters
                root.clone()
            } else {
                ImmutPath::from(root.clean())
            }
        };

        // Try to find an existing entry that we can reuse.
        //
        // We could check with just a read lock, but if the pair is not yet
        // present, we would then need to recheck after acquiring a write lock,
        // which is probably not worth it.
        let mut interner = INTERNER.write();
        if let Some(&id) = interner.to_id.get(&root) {
            return id;
        }

        // Create a new entry forever by leaking the pair. We can't leak more
        // than 2^16 pair (and typically will leak a lot less), so its not a
        // big deal.
        let num = interner.from_id.len().try_into().expect("out of file ids");
        let id = WorkspaceId(num);
        interner.to_id.insert(root.clone(), id);
        interner.from_id.push(root.clone());
        id
    }

    /// Creates a file id for a rootless file.
    pub fn rootless_file(path: VirtualPath) -> FileId {
        FileId::new(None, path)
    }

    /// Creates a file ID for a file with its parent directory as the root.
    pub fn file_with_parent_root(path: &Path) -> Option<FileId> {
        if !path.is_absolute() {
            return None;
        }
        let parent = path.parent()?;
        let parent = ImmutPath::from(parent);
        let path = VirtualPath::new(path.file_name()?);
        Some(Self::workspace_file(Some(&parent), path))
    }

    /// Creates a file ID for a file in a workspace. The `root` is the root
    /// directory of the workspace. If `root` is `None`, the source code at the
    /// `path` will not be able to access physical files.
    pub fn workspace_file(root: Option<&ImmutPath>, path: VirtualPath) -> FileId {
        let workspace = root.map(Self::workspace_id);
        FileId::new(workspace.as_ref().map(WorkspaceId::package), path)
    }

    /// Mounts an untitled file to a workspace. The `root` is the
    /// root directory of the workspace. If `root` is `None`, the source
    /// code at the `path` will not be able to access physical files.
    pub fn rooted_untitled(root: Option<&ImmutPath>, path: VirtualPath) -> FileId {
        let workspace = root.map(Self::workspace_id);
        FileId::new(workspace.as_ref().map(WorkspaceId::untitled_root), path)
    }

    /// Resolves a file ID to its corresponding workspace resolution.
    pub fn resolve(fid: FileId) -> FileResult<WorkspaceResolution> {
        let Some(package) = fid.package() else {
            return Ok(WorkspaceResolution::Rootless);
        };

        match package.namespace.as_str() {
            "ws" => {
                let id = WorkspaceId::from_package_name(&package.name).ok_or_else(|| {
                    FileError::Other(Some(eco_format!("bad workspace id: {fid:?}")))
                })?;

                Ok(if package.version == UNTITLED_ROOT {
                    WorkspaceResolution::UntitledRooted(id)
                } else {
                    WorkspaceResolution::Workspace(id)
                })
            }
            _ => Ok(WorkspaceResolution::Package),
        }
    }

    /// Creates a display wrapper for a file ID that can be formatted for output.
    pub fn display(id: Option<FileId>) -> Resolving {
        Resolving { id }
    }
}

/// A wrapper for displaying file IDs in a human-readable format.
pub struct Resolving {
    id: Option<FileId>,
}

impl fmt::Debug for Resolving {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use WorkspaceResolution::*;
        let Some(id) = self.id else {
            return write!(f, "unresolved-path");
        };

        let path = match WorkspaceResolver::resolve(id) {
            Ok(Workspace(workspace)) => id.vpath().resolve(&workspace.path()),
            Ok(UntitledRooted(..)) => Some(id.vpath().as_rootless_path().to_owned()),
            Ok(Rootless | Package) | Err(_) => None,
        };

        if let Some(path) = path {
            write!(f, "{}", path.display())
        } else {
            write!(f, "{:?}", self.id)
        }
    }
}

impl fmt::Display for Resolving {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use WorkspaceResolution::*;
        let Some(id) = self.id else {
            return write!(f, "unresolved-path");
        };

        let path = match WorkspaceResolver::resolve(id) {
            Ok(Workspace(workspace)) => id.vpath().resolve(&workspace.path()),
            Ok(UntitledRooted(..)) => Some(id.vpath().as_rootless_path().to_owned()),
            Ok(Rootless | Package) | Err(_) => None,
        };

        if let Some(path) = path {
            write!(f, "{}", path.display())
        } else {
            let pkg = id.package();
            match pkg {
                Some(pkg) => {
                    write!(f, "{pkg}{}", id.vpath().as_rooted_path().display())
                }
                None => write!(f, "{}", id.vpath().as_rooted_path().display()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_interner_untitled() {}

    #[test]
    fn test_workspace_id_preserves_uri_roots() {
        let uri_root = ImmutPath::from(PathBuf::from("oct:/workspace/project"));
        let id = WorkspaceResolver::workspace_id(&uri_root);
        let interner = INTERNER.read();
        let stored = interner.from_id.get(id.0 as usize).expect("id present");
        assert_eq!(stored.as_ref().to_string_lossy(), "oct:/workspace/project");
    }

    #[test]
    fn test_workspace_id_cleans_regular_paths() {
        let p = ImmutPath::from(PathBuf::from("/tmp/../tmp/project"));
        let id = WorkspaceResolver::workspace_id(&p);
        let interner = INTERNER.read();
        let stored = interner.from_id.get(id.0 as usize).expect("id present");
        // Normalize separators to make the assertion platform-independent.
        let norm = stored.as_ref().to_string_lossy().replace('\\', "/");
        assert!(norm.contains("/tmp/project") || norm.ends_with("/project"));
        assert!(!norm.contains(".."));
    }
}
