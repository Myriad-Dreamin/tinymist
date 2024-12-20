use std::path::Path;

use reflexo_typst::{vfs::FileId, TypstFileId};
use typst::{diag::FileResult, syntax::Source};

use crate::LspWorld;

// use crate::{analysis::SharedContext, LspWorldExt};

#[salsa::input]
pub struct SalsaFile {
    pub fid: FileId,

    /// The file revision. A file has changed if the revisions don't compare
    /// equal.
    #[default]
    pub revision: usize,
}

#[salsa::input]
pub struct SalsaSource {
    pub fid: TypstFileId,
    #[return_ref]
    pub contents: FileResult<Source>,
}

impl SalsaSource {
    pub fn must_contents(&self, db: &dyn Db) -> Source {
        self.contents(db)
            .as_ref()
            .cloned()
            .ok()
            .unwrap_or_else(|| Source::new(self.fid(db), "".into()))
    }
}

pub trait PathResolver {
    /// Get file's id by its path
    fn file_id_by_path(&self, p: &Path) -> FileResult<TypstFileId>;
}

/// Database which stores all significant input facts: source code and project
/// model. Everything else in tinymist is derived from these queries.
#[salsa_macros::db]
pub trait Db: salsa::Database + PathResolver {
    fn world(&self) -> &LspWorld;
    // fn ctx(&self) -> &Arc<SharedContext>;
    fn source_by_id(&self, id: TypstFileId) -> FileResult<SalsaSource>;
}
