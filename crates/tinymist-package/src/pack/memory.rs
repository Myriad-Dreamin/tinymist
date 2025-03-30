use std::{collections::HashMap, io::Cursor};

use super::*;

/// A package in the directory.
#[derive(Debug, Clone)]
pub struct MapPack {
    /// The package specifier.
    pub specifier: PackageSpec,
    /// The files storing the package.
    pub files: HashMap<String, ImmutBytes>,
}

impl MapPack {
    /// Creates a new `MapPack` instance.
    pub fn new(specifier: PackageSpec, files: HashMap<String, ImmutBytes>) -> Self {
        Self { specifier, files }
    }
}

impl PackFs for MapPack {
    fn read_all(
        &mut self,
        f: &mut (dyn FnMut(&str, PackFile) -> PackageResult<()> + Send + Sync),
    ) -> PackageResult<()> {
        for (path, data) in self.files.iter() {
            let pack_file = PackFile::Data(Cursor::new(data.clone()));
            f(path, pack_file)?;
        }

        Ok(())
    }
}

impl Pack for MapPack {}
