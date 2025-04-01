use std::{collections::HashMap, io::Cursor};

use ecow::EcoString;

use super::*;

/// A package in the directory.
#[derive(Default, Debug, Clone)]
pub struct MapPack {
    /// The files storing the package.
    pub files: HashMap<EcoString, ImmutBytes>,
}

impl MapPack {
    /// Creates a new `MapPack` instance.
    pub fn new(files: HashMap<EcoString, ImmutBytes>) -> Self {
        Self { files }
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
impl PackExt for MapPack {}

impl CloneIntoPack for MapPack {
    fn clone_into_pack(&mut self, pack: &mut impl PackFs) -> std::io::Result<()> {
        pack.read_all(&mut |path, file| {
            let data = match file {
                PackFile::Read(mut reader) => {
                    let mut dst = Vec::new();
                    std::io::copy(&mut reader, &mut dst).map_err(other)?;
                    ImmutBytes::from(dst)
                }
                PackFile::Data(data) => data.into_inner(),
            };
            self.files.insert(path.into(), data);
            Ok(())
        })
        .map_err(other_io)?;
        Ok(())
    }
}
