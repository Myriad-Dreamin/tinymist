//! The package registry of the world.

use std::sync::Arc;

use tinymist_std::ImmutPath;
use typst::diag::FileResult;

pub use tinymist_package::*;

/// A path mapper for the package registry.
pub struct RegistryPathMapper<T> {
    /// The package registry.
    pub registry: Arc<T>,
}

impl<T> RegistryPathMapper<T> {
    /// Creates a new path mapper for the package registry.
    pub fn new(registry: Arc<T>) -> Self {
        Self { registry }
    }
}

impl<T: PackageRegistry> tinymist_vfs::RootResolver for RegistryPathMapper<T> {
    fn resolve_package_root(&self, pkg: &PackageSpec) -> FileResult<ImmutPath> {
        Ok(self.registry.resolve(pkg)?)
    }
}
