use std::{fs::File, io::Read, path::Path};

use typst::diag::{FileError, FileResult};

use crate::{Bytes, PathAccessModel};
use tinymist_std::ReadAllOnce;

/// Provides SystemAccessModel that makes access to the local file system for
/// system compilation.
#[derive(Debug, Clone, Copy)]
pub struct SystemAccessModel;

impl SystemAccessModel {
    fn stat(&self, src: &Path) -> std::io::Result<SystemFileMeta> {
        let meta = std::fs::metadata(src)?;
        Ok(SystemFileMeta {
            is_dir: meta.is_dir(),
        })
    }
}

impl PathAccessModel for SystemAccessModel {
    fn content(&self, src: &Path) -> FileResult<Bytes> {
        let f = |e| FileError::from_io(e, src);
        let mut buf = Vec::<u8>::new();

        let meta = self.stat(src).map_err(f)?;

        if meta.is_dir {
            return Err(FileError::IsDirectory);
        }

        std::fs::File::open(src)
            .map_err(f)?
            .read_to_end(&mut buf)
            .map_err(f)?;
        Ok(buf.into())
    }
}

/// Lazily opened file entry corresponding to a file in the local file system.
///
/// This is used by font loading instead of the [`SystemAccessModel`].
#[derive(Debug)]
pub struct LazyFile {
    path: std::path::PathBuf,
    file: Option<std::io::Result<File>>,
}

impl LazyFile {
    /// Create a new [`LazyFile`] with the given path.
    pub fn new(path: std::path::PathBuf) -> Self {
        Self { path, file: None }
    }
}

impl ReadAllOnce for LazyFile {
    fn read_all(mut self, buf: &mut Vec<u8>) -> std::io::Result<usize> {
        let file = self.file.get_or_insert_with(|| File::open(&self.path));
        let Ok(ref mut file) = file else {
            let err = file.as_ref().unwrap_err();
            // todo: clone error or hide error
            return Err(std::io::Error::new(err.kind(), err.to_string()));
        };

        file.read_to_end(buf)
    }
}

/// Meta data of a file in the local file system.
#[derive(Debug, Clone, Copy)]
pub struct SystemFileMeta {
    is_dir: bool,
}
