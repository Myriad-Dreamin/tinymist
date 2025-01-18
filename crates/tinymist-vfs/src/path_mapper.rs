//! Maps paths to compact integer ids. We don't care about clearings paths which
//! no longer exist -- the assumption is total size of paths we ever look at is
//! not too big.

use std::collections::HashMap;
use std::sync::LazyLock;

use parking_lot::RwLock;
use tinymist_std::path::PathClean;
use tinymist_std::ImmutPath;
use typst::diag::{eco_format, EcoString, FileError, FileResult};
use typst::syntax::package::{PackageSpec, PackageVersion};
use typst::syntax::VirtualPath;

use super::TypstFileId;

pub trait PathMapper {
    fn path_for_id(&self, file_id: TypstFileId) -> FileResult<ImmutPath>;
}

#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct WorkspaceId(u16);

const NO_VERSION: PackageVersion = PackageVersion {
    major: 0,
    minor: 0,
    patch: 0,
};

impl WorkspaceId {
    fn package(&self) -> PackageSpec {
        PackageSpec {
            namespace: WorkspaceResolver::WORKSPACE_NS.clone(),
            name: eco_format!("p{}", self.0),
            version: NO_VERSION,
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

    /// Creates a file id for a file in some workspace. The `root` is the root
    /// directory of the workspace. If `root` is `None`, the source code at the
    /// `path` will not be able to access physical files.
    pub fn workspace_file(root: Option<&ImmutPath>, path: VirtualPath) -> TypstFileId {
        let workspace = root.map(Self::workspace_id);
        TypstFileId::new(workspace.as_ref().map(WorkspaceId::package), path)
    }

    /// File path corresponding to the given `fid`.
    pub fn resolve(fid: TypstFileId) -> FileResult<WorkspaceResolution> {
        let package = fid.package().ok_or_else(|| {
            FileError::Other(Some(eco_format!(
                "cannot map file id without package spec: {fid:?}"
            )))
        })?;

        match package.namespace.as_str() {
            "ws" => {
                let id = WorkspaceId::from_package_name(&package.name).ok_or_else(|| {
                    FileError::Other(Some(eco_format!("bad workspace id: {fid:?}")))
                })?;

                Ok(WorkspaceResolution::Workspace(id))
            }
            _ => Ok(WorkspaceResolution::Package),
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_interner_untitled() {}
}
