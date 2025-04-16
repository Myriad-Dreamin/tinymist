//! Package Registry.

use std::num::NonZeroUsize;
use std::{path::Path, sync::Arc};

use ecow::EcoString;
pub use typst::diag::PackageError;
pub use typst::syntax::package::PackageSpec;

mod dummy;
pub use dummy::*;

#[cfg(feature = "browser")]
mod browser;
#[cfg(feature = "browser")]
pub use browser::*;

#[cfg(feature = "http-registry")]
mod http;
#[cfg(feature = "http-registry")]
pub use http::*;

/// The default Typst registry.
pub const DEFAULT_REGISTRY: &str = "https://packages.typst.org";

/// A trait for package registries.
pub trait PackageRegistry {
    /// A function to be called when the registry is reset.
    fn reset(&mut self) {}

    /// If the state of package registry can be well-defined by a revision, it
    /// should return it. This is used to determine if the compiler should clean
    /// and pull the registry again.
    fn revision(&self) -> Option<NonZeroUsize> {
        None
    }

    /// Resolves a package specification to a local path.
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

/// A trait for package registries that can be notified.
pub trait Notifier {
    /// Called when a package is being downloaded.
    fn downloading(&self, _spec: &PackageSpec) {}
}

/// A dummy notifier that does nothing.
#[derive(Debug, Default, Clone, Copy, Hash)]
pub struct DummyNotifier;

impl Notifier for DummyNotifier {}
