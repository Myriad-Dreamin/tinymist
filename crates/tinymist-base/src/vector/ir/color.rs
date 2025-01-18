use super::preludes::*;

/// Item representing an 8-bit color item.
///
/// It is less precise than [`Color32Item`], but it is more widely supported.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct Rgba8Item {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// A 32-bit color in a specific color space.
/// Note: some backends may not support 32-bit colors.
///
/// See <https://developer.chrome.com/docs/css-ui/high-definition-css-color-guide>
///
/// Detection:
///
/// ```js
/// const hasHighDynamicRange = window.matchMedia('(dynamic-range: high)').matches;
/// const hasP3Color = window.matchMedia('(color-gamut: p3)').matches;
/// ```
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct Color32Item {
    /// The color space.
    pub space: ColorSpace,
    /// The color value.
    pub value: [Scalar; 4],
}

/// A color space for mixing.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub enum ColorSpace {
    /// Luma color space.
    Luma,

    /// A perceptual color space.
    Oklab,

    /// The standard RGB color space.
    Srgb,

    /// The D65-gray color space.
    D65Gray,

    /// The linear RGB color space.
    LinearRgb,

    /// The HSL color space.
    Hsl,

    /// The HSV color space.
    Hsv,

    /// The CMYK color space.
    Cmyk,

    /// The perceptual Oklch color space.
    Oklch,
}

impl ColorSpace {
    pub fn to_str(&self) -> &'static str {
        match self {
            ColorSpace::Luma => "luma",
            ColorSpace::Oklab => "oklab",
            ColorSpace::Srgb => "srgb",
            ColorSpace::D65Gray => "d65-gray",
            ColorSpace::LinearRgb => "linear-rgb",
            ColorSpace::Hsl => "hsl",
            ColorSpace::Hsv => "hsv",
            ColorSpace::Cmyk => "cmyk",
            ColorSpace::Oklch => "oklch",
        }
    }
}

impl fmt::Display for ColorSpace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

/// Item representing an `<gradient/>` element.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct GradientItem {
    /// The path instruction.
    pub stops: Vec<(Rgba8Item, Scalar)>,
    /// Whether to anti-alias the gradient (used for sharp gradients).
    pub anti_alias: bool,
    /// A color space for mixing.
    pub space: ColorSpace,
    /// The gradient kind.
    /// See [`GradientKind`] for more information.
    pub kind: GradientKind,
    /// Additional gradient styles.
    /// See [`GradientStyle`] for more information.
    pub styles: Vec<GradientStyle>,
}

/// Kind of gradients for [`GradientItem`].
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub enum GradientKind {
    /// Angle of a linear gradient.
    Linear(Scalar),
    /// Radius of a radial gradient.
    Radial(Scalar),
    /// Angle of a conic gradient.
    Conic(Scalar),
}

/// Attributes that is applicable to the [`GradientItem`].
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub enum GradientStyle {
    /// Center of a radial or conic gradient.
    Center(Point),
    /// Focal center of a radial gradient.
    FocalCenter(Point),
    /// Focal radius of a radial gradient.
    FocalRadius(Scalar),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct ColorTransform {
    /// The transformation applied to space-sensitive color.
    pub transform: Transform,
    /// The gradient item.
    pub item: Fingerprint,
}
