impl Notifier for DummyNotifier {}
use std::{path::Path, sync::Arc};

use ecow::EcoString;
pub use typst::diag::PackageError;
pub use typst::syntax::package::PackageSpec;

pub mod dummy;

#[cfg(feature = "browser")]
pub mod browser;

#[cfg(feature = "system")]
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

pub trait Notifier {
    fn downloading(&self, _spec: &PackageSpec) {}
}

#[derive(Debug, Default, Clone, Copy, Hash)]
pub struct DummyNotifier;
