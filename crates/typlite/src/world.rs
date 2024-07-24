use std::sync::OnceLock;

use comemo::Prehashed;
use typst::{
    diag::{FileError, FileResult},
    foundations::{Bytes, Datetime},
    text::{Font, FontBook},
    Library, World,
};
use typst_syntax::{FileId, Source};

/// A world for TypstLite.
pub struct LiteWorld {
    main: Source,
    base: &'static LiteBase,
}

impl LiteWorld {
    /// Create a new world for a single test.
    ///
    /// This is cheap because the shared base for all test runs is lazily
    /// initialized just once.
    pub fn new(main: Source) -> Self {
        static BASE: OnceLock<LiteBase> = OnceLock::new();
        Self {
            main,
            base: BASE.get_or_init(LiteBase::default),
        }
    }
}

impl World for LiteWorld {
    fn library(&self) -> &Prehashed<Library> {
        &self.base.library
    }

    fn book(&self) -> &Prehashed<FontBook> {
        &self.base.book
    }

    fn main(&self) -> Source {
        self.main.clone()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotFound(id.vpath().as_rootless_path().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound(id.vpath().as_rootless_path().into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        Some(self.base.fonts[index].clone())
    }

    fn today(&self, _: Option<i64>) -> Option<Datetime> {
        None
    }
}

/// Shared foundation of all lite worlds.
struct LiteBase {
    library: Prehashed<Library>,
    book: Prehashed<FontBook>,
    fonts: Vec<Font>,
}

impl Default for LiteBase {
    fn default() -> Self {
        let fonts: Vec<_> = typst_assets::fonts()
            .flat_map(|data| Font::iter(Bytes::from_static(data)))
            .collect();

        Self {
            library: Prehashed::new(typst::Library::default()),
            book: Prehashed::new(FontBook::from_fonts(&fonts)),
            fonts,
        }
    }
}
