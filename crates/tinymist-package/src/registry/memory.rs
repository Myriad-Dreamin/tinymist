//! Package registry implementation in memory, which could be used in no-std
//! environments, for example, in a typst plugin.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{PackageError, PackageRegistry, PackageSpec};

/// Creates a memory package registry from the builder.
#[derive(Default, Debug)]
pub struct MemoryRegistry(HashMap<PackageSpec, Arc<Path>>);

impl MemoryRegistry {
    /// Adds a memory package.
    pub fn add_memory_package(&mut self, spec: PackageSpec) -> Arc<Path> {
        let package_root: Arc<Path> = PathBuf::from("/internal-packages")
            .join(spec.name.as_str())
            .join(spec.version.to_string())
            .into();

        self.0.insert(spec, package_root.clone());

        package_root
    }
}

impl PackageRegistry for MemoryRegistry {
    /// Resolves a package.
    fn resolve(&self, spec: &PackageSpec) -> Result<Arc<Path>, PackageError> {
        self.0
            .get(spec)
            .cloned()
            .ok_or_else(|| PackageError::NotFound(spec.clone()))
    }
}
