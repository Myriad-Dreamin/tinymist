use core::fmt;
use std::{num::NonZeroUsize, path::PathBuf, sync::Arc};

use typst::text::{Font, FontBook, FontInfo};
use typst::utils::LazyHash;

use super::FontSlot;
use crate::debug_loc::DataSource;

/// A [`FontResolver`] can resolve a font by index.
/// It also provides FontBook for typst to query fonts.
pub trait FontResolver {
    /// An optionally implemented revision function for users, e.g. the `World`.
    ///
    /// A user of [`FontResolver`] will differentiate the `prev` and `next`
    /// revisions to determine if the underlying state of fonts has changed.
    ///
    /// - If either `prev` or `next` is `None`, the world's revision is always
    ///   increased.
    /// - Otherwise, the world's revision is increased if `prev != next`.
    ///
    /// If the revision of fonts is changed, the world will invalidate all
    /// related caches and increase its revision.
    fn revision(&self) -> Option<NonZeroUsize> {
        None
    }

    /// The font book interface for typst.
    fn font_book(&self) -> &LazyHash<FontBook>;

    /// The index parameter is the index of the font in the `FontBook.infos`.
    fn font(&self, index: usize) -> Option<Font>;

    /// Gets a font by its info.
    fn get_by_info(&self, info: &FontInfo) -> Option<Font> {
        self.default_get_by_info(info)
    }

    /// The default implementation of [`FontResolver::get_by_info`].
    fn default_get_by_info(&self, info: &FontInfo) -> Option<Font> {
        // The selected font should at least has the first codepoint in the
        // coverage. We achieve it by querying the font book with `alternative_text`.
        // todo: better font alternative
        let mut alternative_text = 'c';
        if let Some(codepoint) = info.coverage.iter().next() {
            alternative_text = std::char::from_u32(codepoint).unwrap();
        };

        let index = self
            .font_book()
            .select_fallback(Some(info), info.variant, &alternative_text.to_string())
            .unwrap();
        self.font(index)
    }
}

/// The default FontResolver implementation.
///
/// This is constructed by:
/// - The [`crate::font::system::SystemFontSearcher`] on operating systems.
/// - The [`crate::font::web::BrowserFontSearcher`] on browsers.
/// - Otherwise, [`crate::font::pure::MemoryFontBuilder`] in memory.
#[derive(Debug)]
pub struct FontResolverImpl {
    pub(crate) font_paths: Vec<PathBuf>,
    pub(crate) book: LazyHash<FontBook>,
    pub(crate) fonts: Vec<FontSlot>,
}

impl FontResolverImpl {
    pub fn new(font_paths: Vec<PathBuf>, book: FontBook, fonts: Vec<FontSlot>) -> Self {
        Self {
            font_paths,
            book: LazyHash::new(book),
            fonts,
        }
    }

    pub fn new_with_fonts(
        font_paths: Vec<PathBuf>,
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
        }
    }

    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
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

    /// Describe a font by id.
    pub fn describe_font_by_id(&self, id: usize) -> Option<Arc<DataSource>> {
        self.fonts[id].description.clone()
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
            fonts
                .into_iter()
                .map(|(info, slot)| (info.clone(), slot.clone())),
        );
        verse.increment_revision(|verse| verse.set_fonts(Arc::new(new_resolver)));
    }
}
