use std::sync::Arc;

use rayon::iter::ParallelIterator;
use tinymist_std::hash::FxHashMap;
use typst::foundations::Bytes;
use typst::text::{FontBook, FontInfo};

use crate::debug_loc::{DataSource, MemoryDataSource};
use crate::font::{BufferFontLoader, FontResolverImpl, FontSlot};

use super::ReusableFontResolver;

/// A memory font searcher.
#[derive(Debug)]
pub struct MemoryFontSearcher {
    pub prev: FxHashMap<(Bytes, u32), FontSlot>,
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
            prev: FxHashMap::default(),
            book: FontBook::new(),
            fonts: vec![],
        }
    }

    /// Creates an in-memory searcher, also reuses the previous font resources.
    pub fn reuse(resolver: impl ReusableFontResolver) -> Self {
        Self {
            prev: {
                let slots = resolver.slots();
                FxHashMap::from_iter(slots.flat_map(|slot| {
                    let font = slot.get_uninitialized()??;

                    Some(((font.data().clone(), font.index()), slot))
                }))
            },
            book: FontBook::new(),
            fonts: vec![],
        }
    }

    /// Adds an in-memory font.
    pub fn add_memory_font(&mut self, data: Bytes) {
        self.add_memory_fonts(rayon::iter::once(data));
    }

    /// Adds in-memory fonts.
    pub fn add_memory_fonts(&mut self, data: impl ParallelIterator<Item = Bytes>) {
        let source = DataSource::Memory(MemoryDataSource {
            name: "<memory>".to_owned(),
        });
        self.extend_bytes(data.map(|data| (data, Some(source.clone()))));
    }

    /// Adds a number of raw font resources.
    ///
    /// Note: if you would like to reuse font resources across builds, use
    /// [`Self::extend_bytes`] instead.
    pub fn extend(&mut self, items: impl Iterator<Item = (FontInfo, FontSlot)>) {
        for (info, slot) in items {
            self.book.push(info);
            self.fonts.push(slot);
        }
    }

    /// Adds a number of font data to the font resolver. The builder will reuse
    /// the existing font resources according to the bytes.
    pub fn extend_bytes(
        &mut self,
        items: impl ParallelIterator<Item = (Bytes, Option<DataSource>)>,
    ) {
        let loaded = items.flat_map(|(data, desc)| {
            let count = ttf_parser::fonts_in_collection(&data).unwrap_or(1);

            let desc = desc.map(Arc::new);

            (0..count)
                .into_iter()
                .flat_map(|index| {
                    self.prev
                        .get(&(data.clone(), index))
                        .and_then(|s| {
                            let info = s.get_uninitialized()??.info().clone();
                            Some((info, s.clone()))
                        })
                        .or_else(|| {
                            let info = FontInfo::new(&data, index)?;
                            let mut slot = FontSlot::new(BufferFontLoader {
                                buffer: Some(data.clone()),
                                index: index as u32,
                            });
                            if let Some(desc) = desc.clone() {
                                slot = slot.with_describe_arc(desc);
                            }

                            Some((info, slot))
                        })
                })
                .collect::<Vec<_>>()
        });

        self.extend(loaded.collect::<Vec<_>>().into_iter());
    }

    /// Builds a FontResolverImpl.
    pub fn build(self) -> FontResolverImpl {
        FontResolverImpl::new(Vec::new(), self.book, self.fonts)
    }
}

#[deprecated(note = "use [`MemoryFontSearcher`] instead")]
pub type MemoryFontBuilder = MemoryFontSearcher;
