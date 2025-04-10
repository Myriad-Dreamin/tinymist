use std::sync::Arc;

use rayon::iter::ParallelIterator;
use typst::foundations::Bytes;
use typst::text::{FontBook, FontInfo};

use crate::debug_loc::{DataSource, MemoryDataSource};
use crate::font::{BufferFontLoader, FontResolverImpl, FontSlot};

/// A memory font searcher.
#[derive(Debug)]
pub struct MemoryFontSearcher {
    pub fonts: Vec<(FontInfo, FontSlot)>,
}

impl Default for MemoryFontSearcher {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryFontSearcher {
    /// Creates an in-memory searcher.
    pub fn new() -> Self {
        Self { fonts: vec![] }
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
    pub fn extend(&mut self, items: impl IntoIterator<Item = (FontInfo, FontSlot)>) {
        self.fonts.extend(items);
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
                .collect::<Vec<_>>()
        });

        self.extend(loaded.collect::<Vec<_>>());
    }

    /// Builds a FontResolverImpl.
    pub fn build(self) -> FontResolverImpl {
        let slots = self.fonts.iter().map(|(_, slot)| slot.clone()).collect();
        let book = FontBook::from_infos(self.fonts.into_iter().map(|(info, _)| info));
        FontResolverImpl::new(Vec::new(), book, slots)
    }
}

#[deprecated(note = "use [`MemoryFontSearcher`] instead")]
pub type MemoryFontBuilder = MemoryFontSearcher;
