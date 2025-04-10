use typst::foundations::Bytes;
use typst::text::{FontBook, FontInfo};

use crate::debug_loc::{DataSource, MemoryDataSource};
use crate::font::{BufferFontLoader, FontResolverImpl, FontSlot};

/// A memory font builder.
#[derive(Debug)]
pub struct MemoryFontSearcher {
    pub book: FontBook,
    pub fonts: Vec<FontSlot>,
}

impl Default for MemoryFontSearcher {
    fn default() -> Self {
        Self::new()
    }
}

impl From<MemoryFontSearcher> for FontResolverImpl {
    fn from(searcher: MemoryFontSearcher) -> Self {
        FontResolverImpl::new(Vec::new(), searcher.book, searcher.fonts)
    }
}

impl MemoryFontSearcher {
    /// Create a new, empty in-memory searcher.
    pub fn new() -> Self {
        Self {
            book: FontBook::new(),
            fonts: vec![],
        }
    }

    /// Add an in-memory font.
    pub fn add_memory_font(&mut self, data: Bytes) {
        for (index, info) in FontInfo::iter(&data).enumerate() {
            self.book.push(info.clone());
            self.fonts.push(
                FontSlot::new(BufferFontLoader {
                    buffer: Some(data.clone()),
                    index: index as u32,
                })
                .with_describe(DataSource::Memory(MemoryDataSource {
                    name: "<memory>".to_owned(),
                })),
            );
        }
    }
}

#[deprecated(note = "use [`MemoryFontSearcher`] instead")]
pub type MemoryFontBuilder = MemoryFontSearcher;
