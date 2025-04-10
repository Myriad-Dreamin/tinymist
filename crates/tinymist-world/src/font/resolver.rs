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

    /// Gets the font slot by index.
    /// The index parameter is the index of the font in the `FontBook.infos`.
    fn slot(&self, index: usize) -> Option<&FontSlot>;

    /// Gets the font by index.
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

impl<T: FontResolver> FontResolver for Arc<T> {
    fn font_book(&self) -> &LazyHash<FontBook> {
        self.as_ref().font_book()
    }

    fn slot(&self, index: usize) -> Option<&FontSlot> {
        self.as_ref().slot(index)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.as_ref().font(index)
    }

    fn get_by_info(&self, info: &FontInfo) -> Option<Font> {
        self.as_ref().get_by_info(info)
    }
}

pub trait ReusableFontResolver: FontResolver {
    /// Reuses the font resolver.
    fn slots(&self) -> impl Iterator<Item = FontSlot>;
}

impl<T: ReusableFontResolver> ReusableFontResolver for Arc<T> {
    fn slots(&self) -> impl Iterator<Item = FontSlot> {
        self.as_ref().slots()
    }
}

/// The default FontResolver implementation.
///
/// This is constructed by:
/// - The [`crate::font::system::SystemFontSearcher`] on operating systems.
/// - The [`crate::font::web::BrowserFontSearcher`] on browsers.
/// - Otherwise, [`crate::font::pure::MemoryFontBuilder`] in memory.
#[derive(Debug, Default)]
pub struct FontResolverImpl {
    pub(crate) font_paths: Vec<PathBuf>,
    pub(crate) book: LazyHash<FontBook>,
    pub(crate) slots: Vec<FontSlot>,
}

impl FontResolverImpl {
    /// Creates a new font resolver.
    pub fn new(font_paths: Vec<PathBuf>, book: FontBook, slots: Vec<FontSlot>) -> Self {
        Self {
            font_paths,
            book: LazyHash::new(book),
            slots,
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
            slots,
        }
    }

    /// Gets the number of fonts in the resolver.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Tests whether the resolver doesn't hold any fonts.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Gets the user-specified font paths.
    pub fn font_paths(&self) -> &[PathBuf] {
        &self.font_paths
    }

    /// Returns an iterator over all fonts in the resolver.
    #[deprecated(note = "use `fonts` instead")]
    pub fn get_fonts(&self) -> impl Iterator<Item = (&FontInfo, &FontSlot)> {
        self.fonts()
    }

    /// Returns an iterator over all fonts in the resolver.
    pub fn fonts(&self) -> impl Iterator<Item = (&FontInfo, &FontSlot)> {
        self.slots.iter().enumerate().map(|(idx, slot)| {
            let info = self.book.info(idx).unwrap();
            (info, slot)
        })
    }

    /// Returns an iterator over all loaded fonts in the resolver.
    pub fn loaded_fonts(&self) -> impl Iterator<Item = (usize, Font)> + '_ {
        self.slots.iter().enumerate().flat_map(|(idx, slot)| {
            let maybe_font = slot.get_uninitialized().flatten();
            maybe_font.map(|font| (idx, font))
        })
    }

    /// Describes the source of a font.
    pub fn describe_font(&self, font: &Font) -> Option<Arc<DataSource>> {
        let f = Some(Some(font.clone()));
        for slot in &self.slots {
            if slot.get_uninitialized() == f {
                return slot.description.clone();
            }
        }
        None
    }

    /// Describes the source of a font by id.
    pub fn describe_font_by_id(&self, id: usize) -> Option<Arc<DataSource>> {
        self.slots[id].description.clone()
    }

    pub fn with_font_paths(mut self, font_paths: Vec<PathBuf>) -> Self {
        self.font_paths = font_paths;
        self
    }
}

impl FontResolver for FontResolverImpl {
    fn font_book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn slot(&self, idx: usize) -> Option<&FontSlot> {
        self.slots.get(idx)
    }

    fn font(&self, idx: usize) -> Option<Font> {
        self.slots[idx].get_or_init()
    }

    fn get_by_info(&self, info: &FontInfo) -> Option<Font> {
        FontResolver::default_get_by_info(self, info)
    }
}

impl fmt::Display for FontResolverImpl {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (idx, slot) in self.slots.iter().enumerate() {
            writeln!(f, "{:?} -> {:?}", idx, slot.get_uninitialized())?;
        }

        Ok(())
    }
}

impl ReusableFontResolver for FontResolverImpl {
    fn slots(&self) -> impl Iterator<Item = FontSlot> {
        self.slots.iter().cloned()
    }
}
