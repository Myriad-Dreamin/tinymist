impl Notifier for DummyNotifier {}
use std::{path::Path, sync::Arc};

use ecow::EcoString;
use tinymist_std::ImmutPath;
use typst::diag::FileResult;
pub use typst::diag::PackageError;
pub use typst::syntax::package::PackageSpec;

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

impl<T: PackageRegistry> tinymist_vfs::RootResolver for RegistryPathMapper<T> {
    fn resolve_package_root(&self, pkg: &PackageSpec) -> FileResult<ImmutPath> {
        Ok(self.registry.resolve(pkg)?)
    }
}

pub trait Notifier {
    fn downloading(&self, _spec: &PackageSpec) {}
}

#[derive(Debug, Default, Clone, Copy, Hash)]
pub struct DummyNotifier;
