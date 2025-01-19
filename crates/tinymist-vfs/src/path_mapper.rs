//! Maps paths to compact integer ids. We don't care about clearings paths which
//! no longer exist -- the assumption is total size of paths we ever look at is
//! not too big.

use core::fmt;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use parking_lot::RwLock;
use tinymist_std::path::PathClean;
use tinymist_std::ImmutPath;
use typst::diag::{eco_format, EcoString, FileError, FileResult};
use typst::syntax::package::{PackageSpec, PackageVersion};
use typst::syntax::VirtualPath;

use super::TypstFileId;

pub enum PathResolution {
    Resolved(PathBuf),
    Rootless(Cow<'static, VirtualPath>),
}

impl PathResolution {
    pub fn to_err(self) -> FileResult<PathBuf> {
        match self {
            PathResolution::Resolved(path) => Ok(path),
            PathResolution::Rootless(_) => Err(FileError::AccessDenied),
        }
    }

    pub fn as_path(&self) -> &Path {
        match self {
            PathResolution::Resolved(path) => path.as_path(),
            PathResolution::Rootless(path) => path.as_rooted_path(),
        }
    }

    pub fn join(&self, path: &str) -> FileResult<PathResolution> {
        match self {
            PathResolution::Resolved(path) => Ok(PathResolution::Resolved(path.join(path))),
            PathResolution::Rootless(root) => {
                Ok(PathResolution::Rootless(Cow::Owned(root.join(path))))
            }
        }
    }
}

pub trait RootResolver {
    fn path_for_id(&self, file_id: TypstFileId) -> FileResult<PathResolution> {
        use WorkspaceResolution::*;
        let root = match WorkspaceResolver::resolve(file_id)? {
            Workspace(id) => id.path().clone(),
            Package => {
                self.resolve_package_root(file_id.package().expect("not a file in package"))?
            }
            UntitledRooted(..) | Rootless => {
                return Ok(PathResolution::Rootless(Cow::Borrowed(file_id.vpath())))
            }
        };

        file_id
            .vpath()
            .resolve(&root)
            .map(PathResolution::Resolved)
            .ok_or_else(|| FileError::AccessDenied)
    }

    fn resolve_root(&self, file_id: TypstFileId) -> FileResult<Option<ImmutPath>> {
        use WorkspaceResolution::*;
        match WorkspaceResolver::resolve(file_id)? {
            Workspace(id) | UntitledRooted(id) => Ok(Some(id.path().clone())),
            Rootless => Ok(None),
            Package => self
                .resolve_package_root(file_id.package().expect("not a file in package"))
                .map(Some),
        }
    }

    fn resolve_package_root(&self, pkg: &PackageSpec) -> FileResult<ImmutPath>;
}

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

pub enum WorkspaceResolution {
    Workspace(WorkspaceId),
    UntitledRooted(WorkspaceId),
    Rootless,
    Package,
}

/// A package-path interner.
struct Interner {
    to_id: HashMap<ImmutPath, WorkspaceId>,
    from_id: Vec<ImmutPath>,
}

#[derive(Default)]
pub struct WorkspaceResolver {}

impl WorkspaceResolver {
    pub const WORKSPACE_NS: EcoString = EcoString::inline("ws");

    pub fn is_workspace_file(fid: TypstFileId) -> bool {
        fid.package()
            .is_some_and(|p| p.namespace == WorkspaceResolver::WORKSPACE_NS)
    }

    pub fn is_package_file(fid: TypstFileId) -> bool {
        fid.package()
            .is_some_and(|p| p.namespace != WorkspaceResolver::WORKSPACE_NS)
    }

    /// Id of the given path if it exists in the `Vfs` and is not deleted.
    pub fn workspace_id(root: &ImmutPath) -> WorkspaceId {
        // Try to find an existing entry that we can reuse.
        //
        // We could check with just a read lock, but if the pair is not yet
        // present, we would then need to recheck after acquiring a write lock,
        // which is probably not worth it.
        let mut interner = INTERNER.write();
        if let Some(&id) = interner.to_id.get(root) {
            return id;
        }

        let root = ImmutPath::from(root.clean());

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
    pub fn rootless_file(path: VirtualPath) -> TypstFileId {
        TypstFileId::new(None, path)
    }

    /// Creates a file id for a rootless file.
    pub fn file_with_parent_root(path: &Path) -> Option<TypstFileId> {
        if !path.is_absolute() {
            return None;
        }
        let parent = path.parent()?;
        let parent = ImmutPath::from(parent);
        let path = VirtualPath::new(path.file_name()?);
        Some(Self::workspace_file(Some(&parent), path))
    }

    /// Creates a file id for a file in some workspace. The `root` is the root
    /// directory of the workspace. If `root` is `None`, the source code at the
    /// `path` will not be able to access physical files.
    pub fn workspace_file(root: Option<&ImmutPath>, path: VirtualPath) -> TypstFileId {
        let workspace = root.map(Self::workspace_id);
        TypstFileId::new(workspace.as_ref().map(WorkspaceId::package), path)
    }

    /// Mounts an untiled file to some workspace. The `root` is the
    /// root directory of the workspace. If `root` is `None`, the source
    /// code at the `path` will not be able to access physical files.
    pub fn rooted_untitled(root: Option<&ImmutPath>, path: VirtualPath) -> TypstFileId {
        let workspace = root.map(Self::workspace_id);
        TypstFileId::new(workspace.as_ref().map(WorkspaceId::untitled_root), path)
    }

    /// File path corresponding to the given `fid`.
    pub fn resolve(fid: TypstFileId) -> FileResult<WorkspaceResolution> {
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

    /// File path corresponding to the given `fid`.
    pub fn display(id: Option<TypstFileId>) -> Resolving {
        Resolving { id }
    }
}

pub struct Resolving {
    id: Option<TypstFileId>,
}

impl fmt::Debug for Resolving {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use WorkspaceResolution::*;
        let Some(id) = self.id else {
            return write!(f, "None");
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

#[cfg(test)]
mod tests {

    #[test]
    fn test_interner_untitled() {}
}
