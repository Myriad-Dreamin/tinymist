use std::{fmt::Debug, sync::Arc};

use typst::diag::FileResult;

use crate::{path_mapper::RootResolver, AccessModel, Bytes, PathAccessModel, TypstFileId};

/// Provides resolve access model.
#[derive(Clone)]
pub struct ResolveAccessModel<M> {
    pub resolver: Arc<dyn RootResolver + Send + Sync>,
    pub inner: M,
}

impl<M> Debug for ResolveAccessModel<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolveAccessModel").finish()
    }
}

impl<M: PathAccessModel> AccessModel for ResolveAccessModel<M> {
    fn is_file(&self, fid: TypstFileId) -> FileResult<bool> {
        self.inner
            .is_file(&self.resolver.path_for_id(fid)?.to_err()?)
    }

    fn content(&self, fid: TypstFileId) -> FileResult<Bytes> {
        self.inner
            .content(&self.resolver.path_for_id(fid)?.to_err()?)
    }
}
