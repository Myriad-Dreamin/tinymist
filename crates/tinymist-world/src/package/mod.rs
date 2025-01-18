impl Notifier for DummyNotifier {}
use std::{path::Path, sync::Arc};

use ecow::EcoString;
use tinymist_std::ImmutPath;
use tinymist_vfs::{TypstFileId, WorkspaceResolution, WorkspaceResolver};
pub use typst::diag::PackageError;
use typst::diag::{FileError, FileResult};
pub use typst::syntax::package::PackageSpec;

use crate::DETACHED_ENTRY;

pub mod dummy;

#[cfg(feature = "browser")]
pub mod browser;

#[cfg(feature = "http-registry")]
pub mod http;

pub trait PackageRegistry {
    fn reset(&mut self) {}

    fn resolve(&self, spec: &PackageSpec) -> Result<Arc<Path>, PackageError>;

    /// A list of all available packages and optionally descriptions for them.
    ///
    /// This function is optional to implement. It enhances the user experience
    /// by enabling autocompletion for packages. Details about packages from the
    /// `@preview` namespace are available from
    /// `https://packages.typst.org/preview/index.json`.
    fn packages(&self) -> &[(PackageSpec, Option<EcoString>)] {
        &[]
    }
}

pub struct RegistryPathMapper<T> {
    pub registry: Arc<T>,
}

impl<T> RegistryPathMapper<T> {
    pub fn new(registry: Arc<T>) -> Self {
        Self { registry }
    }
}

impl<T: PackageRegistry> tinymist_vfs::PathMapper for RegistryPathMapper<T> {
    fn path_for_id(&self, id: TypstFileId) -> FileResult<ImmutPath> {
        if id == *DETACHED_ENTRY {
            return Ok(DETACHED_ENTRY.vpath().as_rooted_path().into());
        }

        // Determine the root path relative to which the file path
        // will be resolved.
        let root = match WorkspaceResolver::resolve(id)? {
            WorkspaceResolution::Workspace(id) | WorkspaceResolution::Untitled(id) => {
                id.path().clone()
            }
            WorkspaceResolution::Package => self.registry.resolve(id.package().unwrap())?,
        };

        // Join the path to the root. If it tries to escape, deny
        // access. Note: It can still escape via symlinks.
        let path = id.vpath().resolve(&root).map(From::from);
        path.ok_or(FileError::AccessDenied)
    }
}

pub trait Notifier {
    fn downloading(&self, _spec: &PackageSpec) {}
}

#[derive(Debug, Default, Clone, Copy, Hash)]
pub struct DummyNotifier;
