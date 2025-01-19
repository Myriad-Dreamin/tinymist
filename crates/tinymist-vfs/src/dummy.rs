use std::path::Path;

use tinymist_std::ImmutPath;
use typst::diag::{FileError, FileResult};

use crate::{AccessModel, Bytes, PathAccessModel, TypstFileId};

/// Provides dummy access model.
///
/// Note: we can still perform compilation with dummy access model, since
/// [`super::Vfs`] will make a overlay access model over the provided dummy
/// access model.
#[derive(Default, Debug, Clone, Copy)]
pub struct DummyAccessModel;

impl AccessModel for DummyAccessModel {
    fn content(&self, _src: TypstFileId) -> (Option<ImmutPath>, FileResult<Bytes>) {
        (None, Err(FileError::AccessDenied))
    }
}

impl PathAccessModel for DummyAccessModel {
    fn content(&self, _src: &Path) -> FileResult<Bytes> {
        Err(FileError::AccessDenied)
    }
}
