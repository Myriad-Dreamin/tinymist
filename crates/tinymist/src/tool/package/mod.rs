//! Package management tools.

use std::path::PathBuf;

use reflexo_typst::package::{http::HttpRegistry, PackageRegistry, PackageSpec};
use reflexo_typst::typst::prelude::*;
use typst::diag::{eco_format, EcoString, StrResult};
use typst::syntax::package::{PackageVersion, VersionlessPackageSpec};

use crate::LspWorld;

mod init;
pub use init::*;

/// Try to determine the latest version of a package.
pub fn determine_latest_version(
    world: &LspWorld,
    spec: &VersionlessPackageSpec,
) -> StrResult<PackageVersion> {
    if spec.namespace == "preview" {
        let packages = world.registry.packages();
        packages
            .iter()
            .filter(|(package, _)| package.namespace == "preview" && package.name == spec.name)
            .map(|(package, _)| package.version)
            .max()
            .ok_or_else(|| eco_format!("failed to find package {spec}"))
    } else {
        // For other namespaces, search locally. We only search in the data
        // directory and not the cache directory, because the latter is not
        // intended for storage of local packages.
        let subdir = format!("typst/packages/{}/{}", spec.namespace, spec.name);
        world
            .registry
            .local_path()
            .into_iter()
            .flat_map(|dir| std::fs::read_dir(dir.join(&subdir)).ok())
            .flatten()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter_map(|path| path.file_name()?.to_string_lossy().parse().ok())
            .max()
            .ok_or_else(|| eco_format!("please specify the desired version"))
    }
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
        if !local_path.exists() || !local_path.is_dir() {
            continue;
        }
        // namespace/package_name/version
        // 2. package_name
        let package_names = std::fs::read_dir(local_path).unwrap();
        for package in package_names {
            let package = package.unwrap();
            if !package.file_type().unwrap().is_dir() {
                continue;
            }
            if package.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            // 3. version
            let versions = std::fs::read_dir(package.path()).unwrap();
            for version in versions {
                let version = version.unwrap();
                if !version.file_type().unwrap().is_dir() {
                    continue;
                }
                if version.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let path = version.path();
                let version = version.file_name().to_string_lossy().parse().unwrap();
                let spec = PackageSpec {
                    namespace: ns.clone(),
                    name: package.file_name().to_string_lossy().into(),
                    version,
                };
                packages.push((path, spec));
            }
        }
    }

    packages
}
