//! Dummy package registry implementation for testing purposes.

use std::{path::Path, sync::Arc};

use super::{PackageError, PackageRegistry, PackageSpec};

/// Dummy package registry that always returns a `NotFound` error.
#[derive(Default, Debug)]
pub struct DummyRegistry;

impl PackageRegistry for DummyRegistry {
    fn resolve(&self, spec: &PackageSpec) -> Result<Arc<Path>, PackageError> {
        Err(PackageError::NotFound(spec.clone()))
    }
}
