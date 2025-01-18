use super::{preludes::*, ImageItem, PathStyle};
use crate::vector::vm::{GroupContext, TransformContext};

/// The glyph item definition with all of variants of `GlyphItem` other than
/// `GlyphItem::Raw`, hence it is serializable.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub enum FlatGlyphItem {
    None,
    Image(Arc<ImageGlyphItem>),
    Outline(Arc<OutlineGlyphItem>),
}

/// A image glyph item.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct ImageGlyphItem {
    pub ts: Transform,
    pub image: ImageItem,
    pub ligature_len: u8,
}

/// An outline glyph item.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct OutlineGlyphItem {
    pub ts: Option<Box<Transform>>,
    pub d: ImmutStr,
    pub ligature_len: u8,
}

/// Reference a font item in a more friendly format to compress and store
/// information. The fonts are locally stored in the svg module.
/// With a font reference, we can get both the font metric and the font data.
/// The `font_hash` is to let it safe to be cached.
/// By estimation, <https://stackoverflow.com/a/29628053/9323228>
/// If the hash algorithm for `font_hash` is good enough.
/// When you have about 500 fonts (in windows), the collision rate is about:
/// ```plain
/// p(n = 500, d = 2^32) = 1 - exp(-n^2/(2d))
///   = 1 - exp(-500^2/(2*(2^32))) = 0.0000291034
/// ```
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct FontItem {
    /// The hash of the font to avoid global collision.
    // todo: detect collision
    pub fingerprint: Fingerprint,

    pub family: ImmutStr,

    /// The inlined hash of the font to avoid local collision.
    pub hash: u32,
    pub cap_height: Abs,
    pub ascender: Abs,
    pub descender: Abs,
    pub units_per_em: Abs,
    pub vertical: bool,

    pub glyphs: Vec<Arc<FlatGlyphItem>>,

    #[cfg_attr(feature = "rkyv", with(rkyv::with::Skip))]
    pub glyph_cov: bitvec::vec::BitVec<u32>,
}

impl FontItem {
    /// Get a glyph item by its index
    pub fn get_glyph(&self, glyph_id: u32) -> Option<&Arc<FlatGlyphItem>> {
        self.glyphs.get(glyph_id as usize)
    }
}

/// The shape metadata of a [`TextItem`].
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct TextShape {
    /// The font of the text item.
    pub font: FontRef,
    /// The direction of the text item.
    pub dir: ImmutStr,
    /// The size of text
    pub size: Scalar,
    /// The path style.
    /// See [`PathStyle`] for more information.
    pub styles: Vec<PathStyle>,
}

impl TextShape {
    /// ppem is calculated by the font size.
    pub fn ppem(&self, upem: f32) -> Scalar {
        Scalar(self.size.0 / upem)
    }

    /// inv_ppem is calculated by the font size.
    pub fn inv_ppem(&self, upem: f32) -> Scalar {
        Scalar(upem / self.size.0)
    }

    pub fn add_transform<C, T: GroupContext<C> + TransformContext<C>>(
        &self,
        ctx: &mut C,
        group_ctx: T,
        upem: Scalar,
    ) -> T {
        let ppem = self.ppem(upem.0);
        group_ctx.transform_scale(ctx, ppem, -ppem)
    }

    #[inline]
    pub(crate) fn render_glyphs<'a, 'b: 'a>(
        &self,
        upem: Abs,
        glyph_iter: impl Iterator<Item = &'a (Abs, Abs, u32)> + 'a,
        width: &'b mut f32,
    ) -> impl Iterator<Item = (Abs, u32)> + 'a {
        *width = 0f32;

        let inv_ppem = self.inv_ppem(upem.0).0;
        glyph_iter.into_iter().map(move |(offset, advance, glyph)| {
            let offset = *width + offset.0;
            let ts = offset * inv_ppem;

            *width += advance.0;

            (Scalar(ts), *glyph)
        })
    }
}

/// A text item.
/// Item representing an `<g><text/><g/>` element.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct TextItem {
    /// The shape metadata of the text item.
    pub shape: Arc<TextShape>,
    /// The content metadata of the text item.
    pub content: Arc<TextItemContent>,
}

impl TextItem {
    pub fn width(&self) -> Abs {
        Scalar(
            self.content
                .glyphs
                .iter()
                .map(|(_, advance, _)| advance.0)
                .sum(),
        )
    }

    pub fn render_glyphs<'a, 'b: 'a>(
        &'a self,
        upem: Abs,
        width: &'b mut f32,
    ) -> impl Iterator<Item = (Abs, u32)> + 'a {
        self.shape
            .render_glyphs(upem, self.content.glyphs.iter(), width)
    }
}

/// The content metadata of a [`TextItem`].
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct TextItemContent {
    /// The plain utf-8 content of the text item.
    /// Note: without XML escaping.
    pub content: ImmutStr,
    /// The glyphs in the text.
    /// (offset, advance, glyph): ([`Abs`], [`Abs`], [`FlatGlyphItem`])
    pub glyphs: Arc<[(Abs, Abs, u32)]>,
}
