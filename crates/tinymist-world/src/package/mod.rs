use std::sync::Arc;

use tinymist_std::ImmutPath;
use typst::diag::FileResult;

pub use tinymist_package::*;

pub struct RegistryPathMapper<T> {
    pub registry: Arc<T>,
}

impl<T> RegistryPathMapper<T> {
    pub fn new(registry: Arc<T>) -> Self {
        Self { registry }
    }
}

impl<T: PackageRegistry> tinymist_vfs::RootResolver for RegistryPathMapper<T> {
    fn resolve_package_root(&self, pkg: &PackageSpec) -> FileResult<ImmutPath> {
        Ok(self.registry.resolve(pkg)?)
    }
}
