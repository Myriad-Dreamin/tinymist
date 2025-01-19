use std::{path::Path, sync::Arc};

use super::{PackageError, PackageRegistry, PackageSpec};

#[derive(Default, Debug)]
pub struct DummyRegistry;

impl PackageRegistry for DummyRegistry {
    fn resolve(&self, spec: &PackageSpec) -> Result<Arc<Path>, PackageError> {
        Err(PackageError::NotFound(spec.clone()))
    }
}
