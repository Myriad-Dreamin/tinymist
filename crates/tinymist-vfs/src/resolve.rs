use std::{fmt::Debug, sync::Arc};

use tinymist_std::ImmutPath;
use typst::diag::FileResult;

use crate::{path_mapper::RootResolver, AccessModel, Bytes, FileId, PathAccessModel};

/// Provides resolve access model.
#[derive(Clone)]
pub struct ResolveAccessModel<M> {
    /// The path resolver
    pub resolver: Arc<dyn RootResolver + Send + Sync>,
    /// The inner access model
    pub inner: M,
}

impl<M> Debug for ResolveAccessModel<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolveAccessModel").finish()
    }
}

impl<M: PathAccessModel> AccessModel for ResolveAccessModel<M> {
    #[inline]
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn content(&self, fid: FileId) -> (Option<ImmutPath>, FileResult<Bytes>) {
        let resolved = Ok(()).and_then(|_| self.resolver.path_for_id(fid)?.to_err());

        match resolved {
            Ok(path) => (Some(path.as_path().into()), self.inner.content(&path)),
            Err(e) => (None, Err(e)),
        }
    }
}
