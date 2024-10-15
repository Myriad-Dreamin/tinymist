use std::path::PathBuf;

use ecow::EcoString;
use serde::{Deserialize, Serialize};
use tinymist_world::LspWorld;
use typst::diag::{eco_format, StrResult};
use typst::syntax::package::{PackageManifest, PackageSpec};
use typst::syntax::{FileId, VirtualPath};
use typst::World;

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
pub fn get_manifest(world: &LspWorld, toml_id: FileId) -> StrResult<PackageManifest> {
    let toml_data = world
        .file(toml_id)
        .map_err(|err| eco_format!("failed to read package manifest ({})", err))?;

    let string = std::str::from_utf8(&toml_data)
        .map_err(|err| eco_format!("package manifest is not valid UTF-8 ({})", err))?;

    toml::from_str(string)
        .map_err(|err| eco_format!("package manifest is malformed ({})", err.message()))
}

/// Information about a package.
#[derive(Debug, Serialize, Deserialize)]
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
