use core::fmt;
use std::{num::NonZeroUsize, path::PathBuf, sync::Arc};

use typst::text::{Font, FontBook, FontInfo};
use typst::utils::LazyHash;

use super::{FontProfile, FontSlot};
use crate::debug_loc::DataSource;

/// A FontResolver can resolve a font by index.
/// It also reuse FontBook for font-related query.
/// The index is the index of the font in the `FontBook.infos`.
pub trait FontResolver {
    fn revision(&self) -> Option<NonZeroUsize> {
        None
    }

    fn font_book(&self) -> &LazyHash<FontBook>;
    fn font(&self, idx: usize) -> Option<Font>;

    fn default_get_by_info(&self, info: &FontInfo) -> Option<Font> {
        // todo: font alternative
        let mut alternative_text = 'c';
        if let Some(codepoint) = info.coverage.iter().next() {
            alternative_text = std::char::from_u32(codepoint).unwrap();
        };

        let idx = self
            .font_book()
            .select_fallback(Some(info), info.variant, &alternative_text.to_string())
            .unwrap();
        self.font(idx)
    }
    fn get_by_info(&self, info: &FontInfo) -> Option<Font> {
        self.default_get_by_info(info)
    }
}

#[derive(Debug)]
/// The default FontResolver implementation.
pub struct FontResolverImpl {
    font_paths: Vec<PathBuf>,
    book: LazyHash<FontBook>,
    fonts: Vec<FontSlot>,
    profile: FontProfile,
}

impl FontResolverImpl {
    pub fn new(
        font_paths: Vec<PathBuf>,
        book: FontBook,
        fonts: Vec<FontSlot>,
        profile: FontProfile,
    ) -> Self {
        Self {
            font_paths,
            book: LazyHash::new(book),
            fonts,
            profile,
        }
    }

    pub fn new_with_fonts(
        font_paths: Vec<PathBuf>,
        profile: FontProfile,
        fonts: impl Iterator<Item = (FontInfo, FontSlot)>,
    ) -> Self {
        let mut book = FontBook::new();
        let mut slots = Vec::<FontSlot>::new();

        for (info, slot) in fonts {
            book.push(info);
            slots.push(slot);
        }

        Self {
            font_paths,
            book: LazyHash::new(book),
            fonts: slots,
            profile,
        }
    }

    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    pub fn profile(&self) -> &FontProfile {
        &self.profile
    }

    pub fn font_paths(&self) -> &[PathBuf] {
        &self.font_paths
    }

    pub fn loaded_fonts(&self) -> impl Iterator<Item = (usize, Font)> + '_ {
        let slots_with_index = self.fonts.iter().enumerate();

        slots_with_index.flat_map(|(idx, slot)| {
            let maybe_font = slot.get_uninitialized().flatten();
            maybe_font.map(|font| (idx, font))
        })
    }

    pub fn get_fonts(&self) -> impl Iterator<Item = (&FontInfo, &FontSlot)> {
        self.fonts.iter().enumerate().map(|(idx, slot)| {
            let info = self.book.info(idx).unwrap();

            (info, slot)
        })
    }

    pub fn describe_font(&self, font: &Font) -> Option<Arc<DataSource>> {
        let f = Some(Some(font.clone()));
        for slot in &self.fonts {
            if slot.get_uninitialized() == f {
                return slot.description.clone();
            }
        }
        None
    }

    pub fn add_glyph_packs(&mut self) {
        todo!()
    }
}

impl FontResolver for FontResolverImpl {
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

impl fmt::Display for FontResolverImpl {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (idx, slot) in self.fonts.iter().enumerate() {
            writeln!(f, "{:?} -> {:?}", idx, slot.get_uninitialized())?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "system")]
    #[test]
    fn get_fonts_from_system_universe() {
        use clap::Parser as _;

        use crate::args::CompileOnceArgs;

        let args = CompileOnceArgs::parse_from(["tinymist", "main.typ"]);
        let mut verse = args
            .resolve_system()
            .expect("failed to resolve system universe");

        let fonts: Vec<_> = verse.font_resolver.get_fonts().collect();

        let new_resolver = FontResolverImpl::new_with_fonts(
            vec![],
            Default::default(),
            fonts
                .into_iter()
                .map(|(info, slot)| (info.clone(), slot.clone())),
        );
        verse.increment_revision(|verse| verse.set_fonts(Arc::new(new_resolver)));
    }
}
