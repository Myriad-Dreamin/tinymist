use std::{fs::File, io::Write};

use super::*;

/// A package in the directory.
#[derive(Clone)]
pub struct DirPack<P> {
    /// The patch storing the package.
    pub path: P,
}

impl<P: AsRef<Path>> DirPack<P> {
    /// Creates a new `DirPack` instance.
    pub fn new(path: P) -> Self {
        Self { path }
    }
}

impl<P: AsRef<Path>> fmt::Debug for DirPack<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DirPack({})", self.path.as_ref().display())
    }
}

impl<P: AsRef<Path>> PackFs for DirPack<P> {
    fn read_all(
        &mut self,
        f: &mut (dyn FnMut(&str, PackFile) -> PackageResult<()> + Send + Sync),
    ) -> PackageResult<()> {
        self.filter(|_| true).read_all(f)
    }
}

impl<P: AsRef<Path>> Pack for DirPack<P> {}
impl<P: AsRef<Path>> PackExt for DirPack<P> {
    fn filter(&mut self, f: impl Fn(&str) -> bool + Send + Sync) -> impl Pack
    where
        Self: std::marker::Sized,
    {
        FilterDirPack {
            path: &self.path,
            f,
        }
    }
}

impl<P: AsRef<Path>> CloneIntoPack for DirPack<P> {
    fn clone_into_pack(&mut self, pack: &mut impl PackFs) -> std::io::Result<()> {
        let base = self.path.as_ref();
        pack.read_all(&mut |path, file| {
            let path = base.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).map_err(other)?;
            let mut dst = std::fs::File::create(path).map_err(other)?;
            match file {
                PackFile::Read(mut reader) => {
                    std::io::copy(&mut reader, &mut dst).map_err(other)?;
                }
                PackFile::Data(data) => {
                    dst.write_all(&data.into_inner()).map_err(other)?;
                }
            }

            Ok(())
        })
        .map_err(other_io)?;
        Ok(())
    }
}

struct FilterDirPack<'a, P, F> {
    /// The patch storing the package.
    pub path: &'a P,
    /// The filter function.
    pub f: F,
}

impl<S: AsRef<Path>, F> fmt::Debug for FilterDirPack<'_, S, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FilterDirPack({:?}, ..)", self.path.as_ref())
    }
}
impl<Src: AsRef<Path>, F: Fn(&str) -> bool + Send + Sync> PackFs for FilterDirPack<'_, Src, F> {
    fn read_all(
        &mut self,
        f: &mut (dyn FnMut(&str, PackFile) -> PackageResult<()> + Send + Sync),
    ) -> PackageResult<()> {
        let w = walkdir::WalkDir::new(self.path.as_ref())
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file());

        for entry in w {
            let path = entry.path();
            let rel_path = path.strip_prefix(self.path.as_ref()).map_err(other)?;

            let file_path = rel_path.to_string_lossy();
            if !(self.f)(&file_path) {
                continue;
            }

            let pack_file = PackFile::Read(Box::new(File::open(path).map_err(other)?));
            f(&file_path, pack_file)?;
        }

        Ok(())
    }
}

impl<Src: AsRef<Path>, F: Fn(&str) -> bool + Send + Sync> Pack for FilterDirPack<'_, Src, F> {}
