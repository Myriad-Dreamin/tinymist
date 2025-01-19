//! Font resolver implementation.

use core::fmt;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use tinymist_world::font::system::SystemFontSearcher;
use typst::text::{Font, FontBook, FontInfo};
use typst::utils::LazyHash;

use crate::world::vfs::Bytes;
use tinymist_std::debug_loc::DataSource;

pub use crate::world::base::font::*;

#[derive(Debug)]
/// The default FontResolver implementation.
pub struct TinymistFontResolver {
    font_paths: Vec<PathBuf>,
    book: LazyHash<FontBook>,
    partial_book: Arc<Mutex<PartialFontBook>>,
    fonts: Vec<FontSlot>,
}

impl TinymistFontResolver {
    /// Create a new TinymistFontResolver.
    pub fn new(
        font_paths: Vec<PathBuf>,
        book: FontBook,
        partial_book: Arc<Mutex<PartialFontBook>>,
        fonts: Vec<FontSlot>,
    ) -> Self {
        Self {
            font_paths,
            book: LazyHash::new(book),
            partial_book,
            fonts,
        }
    }

    /// Get the number of fonts.
    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    /// Check if the font resolver is empty.
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    /// Get the configured font paths.
    pub fn font_paths(&self) -> &[PathBuf] {
        &self.font_paths
    }

    /// Get the loaded fonts.
    pub fn loaded_fonts(&self) -> impl Iterator<Item = (usize, Font)> + '_ {
        let slots_with_index = self.fonts.iter().enumerate();

        slots_with_index.flat_map(|(idx, slot)| {
            let maybe_font = slot.get_uninitialized().flatten();
            maybe_font.map(|font| (idx, font))
        })
    }

    /// Describe a font.
    pub fn describe_font(&self, font: &Font) -> Option<Arc<DataSource>> {
        let f = Some(Some(font.clone()));
        for slot in &self.fonts {
            if slot.get_uninitialized() == f {
                return slot.description.clone();
            }
        }
        None
    }

    /// Describe a font by id.
    pub fn describe_font_by_id(&self, id: usize) -> Option<Arc<DataSource>> {
        self.fonts[id].description.clone()
    }

    /// Change the font data.
    pub fn modify_font_data(&mut self, idx: usize, buffer: Bytes) {
        let mut font_book = self.partial_book.lock().unwrap();
        for (i, info) in FontInfo::iter(buffer.as_slice()).enumerate() {
            let buffer = buffer.clone();
            let modify_idx = if i > 0 { None } else { Some(idx) };

            font_book.push((
                modify_idx,
                info,
                FontSlot::new(Box::new(BufferFontLoader {
                    buffer: Some(buffer),
                    index: i as u32,
                })),
            ));
        }
    }

    /// Append a font.
    pub fn append_font(&mut self, info: FontInfo, slot: FontSlot) {
        let mut font_book = self.partial_book.lock().unwrap();
        font_book.push((None, info, slot));
    }

    /// Rebuild the font resolver.
    pub fn rebuild(&mut self) {
        let mut partial_book = self.partial_book.lock().unwrap();
        if !partial_book.partial_hit {
            return;
        }
        partial_book.revision += 1;

        let mut book = FontBook::default();

        let mut font_changes = HashMap::new();
        let mut new_fonts = vec![];
        for (idx, info, slot) in partial_book.changes.drain(..) {
            if let Some(idx) = idx {
                font_changes.insert(idx, (info, slot));
            } else {
                new_fonts.push((info, slot));
            }
        }
        partial_book.changes.clear();
        partial_book.partial_hit = false;

        let mut font_slots = Vec::new();
        font_slots.append(&mut self.fonts);
        self.fonts.clear();

        for (i, slot_ref) in font_slots.iter_mut().enumerate() {
            let (info, slot) = if let Some((_, v)) = font_changes.remove_entry(&i) {
                v
            } else {
                book.push(self.book.info(i).unwrap().clone());
                continue;
            };

            book.push(info);
            *slot_ref = slot;
        }

        for (info, slot) in new_fonts.drain(..) {
            book.push(info);
            font_slots.push(slot);
        }

        self.book = LazyHash::new(book);
        self.fonts = font_slots;
    }
}

impl FontResolver for TinymistFontResolver {
    fn font_book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn font(&self, idx: usize) -> Option<Font> {
        self.fonts[idx].get_or_init()
    }

    fn get_by_info(&self, info: &FontInfo) -> Option<Font> {
        FontResolver::default_get_by_info(self, info)
    }
}

impl fmt::Display for TinymistFontResolver {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (idx, slot) in self.fonts.iter().enumerate() {
            writeln!(f, "{:?} -> {:?}", idx, slot.get_uninitialized())?;
        }

        Ok(())
    }
}

impl From<SystemFontSearcher> for TinymistFontResolver {
    fn from(searcher: SystemFontSearcher) -> Self {
        TinymistFontResolver::new(
            searcher.font_paths,
            searcher.book,
            Arc::new(Mutex::new(PartialFontBook::default())),
            searcher.fonts,
        )
    }
}
