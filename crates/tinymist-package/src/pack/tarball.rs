use super::*;

/// A package in the tarball.
pub struct TarballPack<R: ?Sized + Read> {
    /// The holder for the tarball.
    pub reader: tar::Archive<R>,
}

impl<R: Read> TarballPack<R> {
    /// Creates a new `TarballPack` instance.
    pub fn new(reader: R) -> Self {
        let reader = tar::Archive::new(reader);
        Self { reader }
    }
}

impl<R: ?Sized + Read> fmt::Debug for TarballPack<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TarballPack").finish()
    }
}

impl<R: Read> PackFs for TarballPack<R> {
    fn read_all(
        &mut self,
        f: &mut (dyn FnMut(&str, PackFile) -> PackageResult<()> + Send + Sync),
    ) -> PackageResult<()> {
        for entry in self.reader.entries().map_err(malform)? {
            let entry = entry.map_err(malform)?;
            let header = entry.header();

            let is_file = header.entry_type().is_file();
            if !is_file {
                continue;
            }

            let path = header.path().map_err(malform)?;
            let path = path.to_string_lossy().to_string();

            let pack_file = PackFile::Read(Box::new(entry));
            f(&path, pack_file)?;
        }

        Ok(())
    }
}
