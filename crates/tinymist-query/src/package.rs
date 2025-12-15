//! Package management tools.

use std::path::PathBuf;

use ecow::eco_format;
#[cfg(feature = "local-registry")]
use ecow::{EcoVec, eco_vec};
// use reflexo_typst::typst::prelude::*;
use serde::{Deserialize, Serialize};
use tinymist_world::package::PackageSpec;
use tinymist_world::package::registry::PackageIndexEntry;
use typst::World;
use typst::diag::{EcoString, StrResult};
use typst::syntax::package::PackageManifest;
use typst::syntax::{FileId, VirtualPath};

use crate::LocalContext;

/// Information about a package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    /// The path to the package if any.
    pub path: PathBuf,
    /// The namespace the package lives in.
    pub namespace: EcoString,
    /// The name of the package within its namespace.
    pub name: EcoString,
    /// The package's version.
    pub version: String,
}

impl From<PackageIndexEntry> for PackageInfo {
    fn from(entry: PackageIndexEntry) -> Self {
        let spec = entry.spec();
        Self {
            path: entry.path.unwrap_or_default(),
            namespace: spec.namespace,
            name: spec.name,
            version: spec.version.to_string(),
        }
    }
}

/// Parses the manifest of the package located at `package_path`.
pub fn get_manifest_id(spec: &PackageInfo) -> StrResult<FileId> {
    Ok(FileId::new(
        Some(PackageSpec {
            namespace: spec.namespace.clone(),
            name: spec.name.clone(),
            version: spec.version.parse()?,
        }),
        VirtualPath::new("typst.toml"),
    ))
}

/// Parses the manifest of the package located at `package_path`.
pub fn get_manifest(world: &dyn World, toml_id: FileId) -> StrResult<PackageManifest> {
    let toml_data = world
        .file(toml_id)
        .map_err(|err| eco_format!("failed to read package manifest ({})", err))?;

    let string = std::str::from_utf8(&toml_data)
        .map_err(|err| eco_format!("package manifest is not valid UTF-8 ({})", err))?;

    toml::from_str(string)
        .map_err(|err| eco_format!("package manifest is malformed ({})", err.message()))
}

/// Check Package.
pub fn check_package(ctx: &mut LocalContext, spec: &PackageInfo) -> StrResult<()> {
    let toml_id = get_manifest_id(spec)?;
    let manifest = ctx.get_manifest(toml_id)?;

    let entry_point = toml_id.join(&manifest.package.entrypoint);

    ctx.shared_().preload_package(entry_point);
    Ok(())
}

/// A filter for packages.
#[cfg(feature = "local-registry")]
pub enum PackageFilter {
    /// Filter for packages that match the given namespace.
    For(EcoString),
    /// Filter for packages that do not match the given namespace.
    ExceptFor(EcoString),
    /// Filter that matches all packages.
    All,
}

#[cfg(feature = "local-registry")]
/// Get the packages in namespaces and their descriptions.
pub fn list_package(
    world: &tinymist_project::LspWorld,
    filter: PackageFilter,
) -> EcoVec<PackageIndexEntry> {
    trait IsDirFollowLinks {
        fn is_dir_follow_links(&self) -> bool;
    }

    impl IsDirFollowLinks for PathBuf {
        fn is_dir_follow_links(&self) -> bool {
            // Although `canonicalize` is heavy, we must use it because `symlink_metadata`
            // is not reliable.
            self.canonicalize()
                .map(|meta| meta.is_dir())
                .unwrap_or(false)
        }
    }

    let registry = &world.registry;

    // search packages locally. We only search in the data
    // directory and not the cache directory, because the latter is not
    // intended for storage of local packages.
    let mut packages = eco_vec![];

    let paths = registry.paths();
    log::info!("searching for packages in paths {paths:?}");

    let mut search_in_dir = |local_path: PathBuf, ns: EcoString| {
        if !local_path.exists() || !local_path.is_dir_follow_links() {
            return;
        }
        // namespace/package_name/version
        // 2. package_name
        let Some(package_names) = once_log(std::fs::read_dir(local_path), "read local package")
        else {
            return;
        };
        for package in package_names {
            let Some(package) = once_log(package, "read package name") else {
                continue;
            };
            let package_name = EcoString::from(package.file_name().to_string_lossy());
            if package_name.starts_with('.') {
                continue;
            }

            let package_path = package.path();
            if !package_path.is_dir_follow_links() {
                continue;
            }
            // 3. version
            let Some(versions) = once_log(std::fs::read_dir(package_path), "read package versions")
            else {
                continue;
            };
            for version in versions {
                let Some(version_entry) = once_log(version, "read package version") else {
                    continue;
                };
                if version_entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let package_version_path = version_entry.path();
                if !package_version_path.is_dir_follow_links() {
                    continue;
                }
                let Some(version) = once_log(
                    version_entry.file_name().to_string_lossy().parse(),
                    "parse package version",
                ) else {
                    continue;
                };
                let spec = PackageSpec {
                    namespace: ns.clone(),
                    name: package_name.clone(),
                    version,
                };
                let manifest_id = typst::syntax::FileId::new(
                    Some(spec.clone()),
                    typst::syntax::VirtualPath::new("typst.toml"),
                );
                let Some(manifest) =
                    once_log(get_manifest(world, manifest_id), "read package manifest")
                else {
                    continue;
                };
                packages.push(PackageIndexEntry {
                    namespace: ns.clone(),
                    package: manifest.package,
                    template: manifest.template,
                    updated_at: None,
                    path: Some(package_version_path),
                });
            }
        }
    };

    for dir in paths {
        let matching_ns = match &filter {
            PackageFilter::For(ns) => {
                let local_path = dir.join(ns.as_str());
                search_in_dir(local_path, ns.clone());

                continue;
            }
            PackageFilter::ExceptFor(ns) => Some(ns),
            PackageFilter::All => None,
        };

        let Some(namespaces) = once_log(std::fs::read_dir(dir), "read package directory") else {
            continue;
        };
        for dir in namespaces {
            let Some(dir) = once_log(dir, "read ns directory") else {
                continue;
            };
            let ns = dir.file_name();
            let ns = ns.to_string_lossy();
            if let Some(matching_ns) = &matching_ns {
                if matching_ns.as_str() != ns.as_ref() {
                    continue;
                }
            }
            let local_path = dir.path();
            search_in_dir(local_path, ns.into());
        }
    }

    packages
}

#[cfg(feature = "local-registry")]
fn once_log<T, E: std::fmt::Display>(result: Result<T, E>, site: &'static str) -> Option<T> {
    use std::collections::HashSet;
    use std::sync::OnceLock;

    use parking_lot::Mutex;

    let err = match result {
        Ok(value) => return Some(value),
        Err(err) => err,
    };

    static ONCE: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut once = ONCE.get_or_init(Default::default).lock();
    if once.insert(site) {
        log::error!("failed to perform {site}: {err}");
    }

    None
}
