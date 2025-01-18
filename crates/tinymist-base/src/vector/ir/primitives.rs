use base64::Engine;

use super::preludes::*;

/// Create a xml id from the given prefix and the def id of this reference.
/// Note that the def id may not be stable across compilation.
/// Note that the entire html document shares namespace for ids.
pub fn as_svg_id(b: &[u8], prefix: &'static str) -> String {
    // truncate zero
    let rev_zero = b.iter().rev().skip_while(|&&b| b == 0).count();
    let id = &b[..rev_zero];
    let id = base64::engine::general_purpose::STANDARD_NO_PAD.encode(id);
    [prefix, &id].join("")
}

/// A span location in the source code.
/// Note: it is unsafe to transfer a span across processes.
/// Note: a span id is only ensured correct within the same compilation
/// lifespan.
pub type SpanId = u64;

/// The local id of a svg item.
/// This id is only unique within the svg document.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct DefId(pub u64);

impl DefId {
    /// Create a xml id from the given prefix and the def id of this reference.
    /// Note that the def id may not be stable across compilation.
    /// Note that the entire html document shares namespace for ids.
    #[comemo::memoize]
    pub fn as_svg_id(self, prefix: &'static str) -> String {
        as_svg_id(self.0.to_le_bytes().as_slice(), prefix)
    }
}

/// A stable absolute reference.
/// The fingerprint is used to identify the item and likely unique between
/// different svg documents. The (local) def id is only unique within the svg
/// document.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct AbsoluteRef {
    /// The fingerprint of the item.
    pub fingerprint: Fingerprint,
    /// The local def id of the item.
    pub id: DefId,
}

impl fmt::Debug for AbsoluteRef {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "<AbsRef: {}{}>",
            self.fingerprint.as_svg_id(""),
            self.id.0
        )
    }
}

impl Hash for AbsoluteRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.fingerprint.hash(state);
    }
}

impl AbsoluteRef {
    #[inline]
    pub fn as_svg_id(&self, prefix: &'static str) -> String {
        self.fingerprint.as_svg_id(prefix)
    }

    #[inline]
    pub fn as_unstable_svg_id(&self, prefix: &'static str) -> String {
        self.id.as_svg_id(prefix)
    }
}

/// Reference a font item in a more friendly format to compress and store
/// information, similar to [`GlyphRef`].
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct FontRef {
    /// The hash of the font to avoid collision.
    pub hash: u32,
    /// The local id of the font.
    pub idx: u32,
}

/// Reference a glyph item in a more friendly format to compress and store
/// information. The glyphs are locally stored in the svg module.
/// With a glyph reference, we can get both the font metric and the glyph data.
/// The `font_hash` is to let it safe to be cached, please see
/// [`crate::vector::ir::FontItem`] for more details.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct GlyphRef {
    /// The hash of the font to avoid collision.
    pub font_hash: u32,
    /// The local id of the glyph.
    pub glyph_idx: u32,
}

impl GlyphRef {
    #[comemo::memoize]
    pub fn as_svg_id(&self, prefix: &'static str) -> String {
        let t = ((self.font_hash as u64) | ((self.glyph_idx as u64) << 32)).to_le_bytes();
        as_svg_id(&t.as_slice()[..6], prefix)
    }
}
