use typst::diag::{eco_format, StrResult};
use typst::syntax::package::{PackageVersion, VersionlessPackageSpec};
use typst_ts_compiler::package::Registry;

use crate::world::LspWorld;

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
