//! Package management tools.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use parking_lot::Mutex;
use reflexo_typst::typst::prelude::*;
use reflexo_typst::{package::PackageSpec, TypstFileId};
use serde::{Deserialize, Serialize};
use tinymist_world::package::http::HttpRegistry;
use typst::diag::{EcoString, StrResult};
use typst::syntax::package::PackageManifest;
use typst::syntax::VirtualPath;
use typst::World;

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

impl From<(PathBuf, PackageSpec)> for PackageInfo {
    fn from((path, spec): (PathBuf, PackageSpec)) -> Self {
        Self {
            path,
            namespace: spec.namespace,
            name: spec.name,
            version: spec.version.to_string(),
        }
    }
}

/// Parses the manifest of the package located at `package_path`.
pub fn get_manifest_id(spec: &PackageInfo) -> StrResult<TypstFileId> {
    Ok(TypstFileId::new(
        Some(PackageSpec {
            namespace: spec.namespace.clone(),
            name: spec.name.clone(),
            version: spec.version.parse()?,
        }),
        VirtualPath::new("typst.toml"),
    ))
}

/// Parses the manifest of the package located at `package_path`.
pub fn get_manifest(world: &dyn World, toml_id: TypstFileId) -> StrResult<PackageManifest> {
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

/// Get the packages in namespaces and their descriptions.
pub fn list_package_by_namespace(
    registry: &HttpRegistry,
    ns: EcoString,
) -> EcoVec<(PathBuf, PackageSpec)> {
    // search packages locally. We only search in the data
    // directory and not the cache directory, because the latter is not
    // intended for storage of local packages.
    let mut packages = eco_vec![];

    log::info!(
        "searching for packages in namespace {ns} in paths {:?}",
        registry.paths()
    );
    for dir in registry.paths() {
        let local_path = dir.join(ns.as_str());
        if !local_path.exists() || !local_path.is_dir_follow_links() {
            continue;
        }
        // namespace/package_name/version
        // 2. package_name
        let Some(package_names) = once_log(std::fs::read_dir(local_path), "read local package")
        else {
            continue;
        };
        for package in package_names {
            let Some(package) = once_log(package, "read package name") else {
                continue;
            };
            if package.file_name().to_string_lossy().starts_with('.') {
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
                let Some(version) = once_log(version, "read package version") else {
                    continue;
                };
                if version.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let package_version_path = version.path();
                if !package_version_path.is_dir_follow_links() {
                    continue;
                }
                let Some(version) = once_log(
                    version.file_name().to_string_lossy().parse(),
                    "parse package version",
                ) else {
                    continue;
                };
                let spec = PackageSpec {
                    namespace: ns.clone(),
                    name: package.file_name().to_string_lossy().into(),
                    version,
                };
                packages.push((package_version_path, spec));
            }
        }
    }

    packages
}

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

fn once_log<T, E: std::fmt::Display>(result: Result<T, E>, site: &'static str) -> Option<T> {
    let err = match result {
        Ok(value) => return Some(value),
        Err(err) => err,
    };

    static ONCES: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut onces = ONCES.get_or_init(Default::default).lock();
    if onces.insert(site) {
        log::error!("failed to perform {site}: {err}");
    }

    None
}
