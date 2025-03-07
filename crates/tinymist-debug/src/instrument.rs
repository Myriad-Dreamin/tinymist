//! Tinymist instrument support for Typst.

use std::sync::Arc;

use parking_lot::Mutex;
use tinymist_std::hash::FxHashMap;
use tinymist_world::{vfs::FileId, CompilerFeat, CompilerWorld};
use typst::diag::FileResult;
use typst::foundations::{Bytes, Datetime};
use typst::syntax::Source;
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::Library;

pub trait Instrumenter: Send + Sync {
    fn instrument(&self, source: Source) -> FileResult<Source>;
}

pub struct InstrumentWorld<'a, F: CompilerFeat, I> {
    pub base: &'a CompilerWorld<F>,
    pub library: Arc<LazyHash<Library>>,
    pub instr: I,
    pub instrumented: Mutex<FxHashMap<FileId, FileResult<Source>>>,
}

impl<F: CompilerFeat, I: Instrumenter> typst::World for InstrumentWorld<'_, F, I>
where
    I:,
{
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.base.book()
    }

    fn main(&self) -> FileId {
        self.base.main()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        let mut instrumented = self.instrumented.lock();
        if let Some(source) = instrumented.get(&id) {
            return source.clone();
        }

        let source = self.base.source(id).and_then(|s| self.instr.instrument(s));
        instrumented.insert(id, source.clone());
        source
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.base.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.base.font(index)
    }

    fn today(&self, offset: Option<i64>) -> Option<Datetime> {
        self.base.today(offset)
    }
}
