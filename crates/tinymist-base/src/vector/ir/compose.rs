use crate::hash::Fingerprint;

use super::{preludes::*, text::*, VecItem};

/// References to a page frame.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct Page {
    /// Unique hash to content
    pub content: Fingerprint,
    /// Page size for cropping content
    pub size: Size,
}

impl fmt::Debug for Page {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Page({}, {:.3}x{:.3})",
            self.content.as_svg_id(""),
            self.size.x.0,
            self.size.y.0
        )
    }
}

/// References to a vec item with transform.
/// Item representing an `<g/>` element applied with a [`TransformItem`].
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct TransformedRef(pub TransformItem, pub Fingerprint);

/// References to a vec item with transform.
/// Item representing an `<g/>` element applied with a [`TransformItem`].
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct LabelledRef(pub ImmutStr, pub Fingerprint);

/// References to a group of items with translates.
/// Absolute positioning items at their corresponding points.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct GroupRef(pub Arc<[(Point, Fingerprint)]>);

/// References to a set of fonts.
pub type FontPack = Vec<FontItem>;

/// References to a set of glyphs.
pub type GlyphPack = Vec<(GlyphRef, FlatGlyphItem)>;

/// References to a set of items.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct ItemPack(pub Vec<(Fingerprint, VecItem)>);

/// Flatten mapping fingerprints to glyph items.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct IncrFontPack {
    pub items: FontPack,
    pub incremental_base: usize,
}

impl From<FontPack> for IncrFontPack {
    fn from(items: FontPack) -> Self {
        Self {
            items,
            incremental_base: 0,
        }
    }
}

/// Flatten mapping fingerprints to glyph items.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct IncrGlyphPack {
    pub items: GlyphPack,
    pub incremental_base: usize,
}

impl From<GlyphPack> for IncrGlyphPack {
    fn from(items: GlyphPack) -> Self {
        Self {
            items,
            incremental_base: 0,
        }
    }
}

impl FromIterator<(GlyphRef, FlatGlyphItem)> for IncrGlyphPack {
    fn from_iter<T: IntoIterator<Item = (GlyphRef, FlatGlyphItem)>>(iter: T) -> Self {
        Self {
            items: iter.into_iter().collect(),
            incremental_base: 0,
        }
    }
}
