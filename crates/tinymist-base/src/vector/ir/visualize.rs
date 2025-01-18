use super::preludes::*;
use crate::StaticHash128;

/// Item representing an `<image/>` element.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct ImageItem {
    /// The source image data.
    pub image: Arc<Image>,
    /// The target size of the image.
    pub size: Size,
}

/// Item representing an `<image/>` element.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct HtmlItem {
    /// The sanitized source code.
    pub html: ImmutStr,
    /// The target size of the image.
    pub size: Size,
}

/// Data of an `<image/>` element.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct Image {
    /// The encoded image data.
    pub data: Vec<u8>,
    /// The format of the encoded `buffer`.
    pub format: ImmutStr,
    /// The size of the image.
    pub size: Axes<u32>,
    /// A text describing the image.
    pub alt: Option<ImmutStr>,
    /// prehashed image content.
    pub hash: Fingerprint,
}

impl Image {
    /// Returns the width of the image.
    pub fn width(&self) -> u32 {
        self.size.x
    }
    /// Returns the height of the image.
    pub fn height(&self) -> u32 {
        self.size.y
    }
}

/// Prehashed image data.
impl Hash for Image {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.hash.hash(state);
    }
}

impl StaticHash128 for Image {
    /// Returns the hash of the image data.
    fn get_hash(&self) -> u128 {
        self.hash.to_u128()
    }
}

/// Item representing an `<path/>` element.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct PathItem {
    /// The path instruction.
    pub d: ImmutStr,
    /// bbox of the path.
    pub size: Option<Size>,
    /// The path style.
    /// See [`PathStyle`] for more information.
    pub styles: Vec<PathStyle>,
}

/// Attributes that is applicable to the [`PathItem`].
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub enum PathStyle {
    /// `fill` attribute.
    /// See <https://developer.mozilla.org/en-US/docs/Web/SVG/Attribute/fill>
    Fill(ImmutStr),

    /// `stroke` attribute.
    /// See <https://developer.mozilla.org/en-US/docs/Web/SVG/Attribute/stroke>
    Stroke(ImmutStr),

    /// `stroke-linecap` attribute.
    /// See <https://developer.mozilla.org/en-US/docs/Web/SVG/Attribute/stroke-linecap>
    StrokeLineCap(ImmutStr),

    /// `stroke-linejoin` attribute.
    /// See <https://developer.mozilla.org/en-US/docs/Web/SVG/Attribute/stroke-linejoin>
    StrokeLineJoin(ImmutStr),

    /// `stroke-miterlimit` attribute.
    /// See <https://developer.mozilla.org/en-US/docs/Web/SVG/Attribute/stroke-miterlimit>
    StrokeMitterLimit(Scalar),

    /// `stroke-dashoffset` attribute.
    /// See <https://developer.mozilla.org/en-US/docs/Web/SVG/Attribute/stroke-dashoffset>
    StrokeDashOffset(Abs),

    /// `stroke-dasharray` attribute.
    /// See <https://developer.mozilla.org/en-US/docs/Web/SVG/Attribute/stroke-dasharray>
    StrokeDashArray(Arc<[Abs]>),

    /// `stroke-width` attribute.
    /// See <https://developer.mozilla.org/en-US/docs/Web/SVG/Attribute/stroke-width>
    StrokeWidth(Abs),

    /// `fill-rule` attribute.
    /// See <https://developer.mozilla.org/en-US/docs/Web/SVG/Attribute/fill-rule>
    FillRule(ImmutStr),
}

/// Item representing an `<pattern/>` element.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct PatternItem {
    /// The pattern's rendered content.
    pub frame: Fingerprint,
    /// The pattern's tile size.
    pub size: Size,
    /// The pattern's tile spacing.
    pub spacing: Size,
}
