//! Package Registry.

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::{path::Path, sync::Arc};

use ecow::EcoString;
use serde::Deserialize;
use tinymist_std::time::UtcDateTime;
pub use typst::diag::PackageError;
pub use typst::syntax::package::PackageSpec;
use typst::syntax::package::{PackageInfo, TemplateInfo, VersionlessPackageSpec};

mod dummy;
pub use dummy::*;

mod memory;
pub use memory::*;

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

/// The namespace for Typst registry.
pub const PREVIEW_NS: &str = "preview";

/// An extension trait for package specifications.
pub trait PackageSpecExt {
    /// Returns true if the package spec is in the preview namespace.
    fn is_preview(&self) -> bool;
}

impl PackageSpecExt for PackageSpec {
    fn is_preview(&self) -> bool {
        self.namespace == PREVIEW_NS
    }
}

impl PackageSpecExt for VersionlessPackageSpec {
    fn is_preview(&self) -> bool {
        self.namespace == PREVIEW_NS
    }
}

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
    fn packages(&self) -> &[PackageIndexEntry] {
        &[]
    }
}

/// An entry in the package index.
#[derive(Debug, Clone, Deserialize)]
pub struct PackageIndexEntry {
    /// The namespace the package lives in.
    #[serde(default)]
    pub namespace: EcoString,
    /// Details about the package itself.
    #[serde(flatten)]
    pub package: PackageInfo,
    /// Details about the template, if the package is one.
    #[serde(default)]
    pub template: Option<TemplateInfo>,
    /// The timestamp when the package was last updated.
    #[serde(rename = "updatedAt", deserialize_with = "deserialize_timestamp")]
    pub updated_at: Option<UtcDateTime>,
    /// The local path of the package, if available.
    #[serde(default)]
    pub path: Option<PathBuf>,
}

impl PackageIndexEntry {
    /// Get the package specification for this entry.
    pub fn spec(&self) -> PackageSpec {
        PackageSpec {
            namespace: self.namespace.clone(),
            name: self.package.name.clone(),
            version: self.package.version,
        }
    }

    /// Check if this entry matches a versionless package specification.
    pub fn matches_versionless(&self, spec: &VersionlessPackageSpec) -> bool {
        self.namespace == spec.namespace && self.package.name == spec.name
    }
}

fn deserialize_timestamp<'de, D>(deserializer: D) -> Result<Option<UtcDateTime>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let timestamp = i64::deserialize(deserializer)?;
    Ok(UtcDateTime::from_unix_timestamp(timestamp).ok())
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
