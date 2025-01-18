use std::path::Path;

use reflexo::ImmutPath;
use typst::diag::{FileError, FileResult};

use super::AccessModel;
use crate::{Bytes, Time};

/// Provides dummy access model.
///
/// Note: we can still perform compilation with dummy access model, since
/// [`super::Vfs`] will make a overlay access model over the provided dummy
/// access model.
#[derive(Default, Debug, Clone, Copy)]
pub struct DummyAccessModel;

impl AccessModel for DummyAccessModel {
    fn mtime(&self, _src: &Path) -> FileResult<Time> {
        Ok(Time::UNIX_EPOCH)
    }

    fn is_file(&self, _src: &Path) -> FileResult<bool> {
        Ok(true)
    }

    fn real_path(&self, src: &Path) -> FileResult<ImmutPath> {
        Ok(src.into())
    }

    fn content(&self, _src: &Path) -> FileResult<Bytes> {
        Err(FileError::AccessDenied)
    }
}
