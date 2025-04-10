use rayon::iter::ParallelIterator;
use typst::foundations::Bytes;
use typst::text::{FontBook, FontInfo};

use crate::debug_loc::{DataSource, MemoryDataSource};
use crate::font::{BufferFontLoader, FontResolverImpl, FontSlot};

/// A memory font searcher.
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

impl MemoryFontSearcher {
    /// Creates an in-memory searcher.
    pub fn new() -> Self {
        Self {
            book: FontBook::new(),
            fonts: vec![],
        }
    }

    /// Creates an in-memory searcher, also reuses the previous font resources.
    pub fn reuse(resolver: FontResolverImpl) -> Self {
        let _ = resolver;
        Self {
            book: FontBook::new(),
            fonts: vec![],
        }
    }

    /// Adds an in-memory font.
    pub fn add_memory_font(&mut self, data: Bytes) {
        for (index, info) in FontInfo::iter(&data).enumerate() {
            self.book.push(info);
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

    /// Adds in-memory fonts.
    pub fn add_memory_fonts(&mut self, data: impl ParallelIterator<Item = Bytes>) {
        let loaded = data.flat_map(|data| {
            FontInfo::iter(&data)
                .enumerate()
                .map(|(index, info)| {
                    (
                        info,
                        FontSlot::new(BufferFontLoader {
                            buffer: Some(data.clone()),
                            index: index as u32,
                        })
                        .with_describe(DataSource::Memory(
                            MemoryDataSource {
                                name: "<memory>".to_owned(),
                            },
                        )),
                    )
                })
                .collect::<Vec<_>>()
        });

        self.extend(loaded.collect::<Vec<_>>().into_iter());
    }

    pub fn extend(&mut self, items: impl Iterator<Item = (FontInfo, FontSlot)>) {
        for (info, slot) in items {
            self.book.push(info);
            self.fonts.push(slot);
        }
    }

    /// Builds a FontResolverImpl.
    pub fn build(self) -> FontResolverImpl {
        FontResolverImpl::new(Vec::new(), self.book, self.fonts)
    }
}

#[deprecated(note = "use [`MemoryFontSearcher`] instead")]
pub type MemoryFontBuilder = MemoryFontSearcher;
