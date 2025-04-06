use typst::foundations::Bytes;
use typst::text::{FontBook, FontInfo};

use crate::debug_loc::{DataSource, MemoryDataSource};
use crate::font::{BufferFontLoader, FontResolverImpl, FontSlot};

/// memory font builder.
#[derive(Debug)]
pub struct MemoryFontBuilder {
    pub book: FontBook,
    pub fonts: Vec<FontSlot>,
}

impl Default for MemoryFontBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl From<MemoryFontBuilder> for FontResolverImpl {
    fn from(searcher: MemoryFontBuilder) -> Self {
        FontResolverImpl::new(
            Vec::new(),
            searcher.book,
            Default::default(),
            searcher.fonts,
            Default::default(),
        )
    }
}

impl MemoryFontBuilder {
    /// Create a new, empty system searcher.
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
                FontSlot::new_boxed(BufferFontLoader {
                    buffer: Some(data.clone()),
                    index: index as u32,
                })
                .describe(DataSource::Memory(MemoryDataSource {
                    name: "<memory>".to_owned(),
                })),
            );
        }
    }
}
