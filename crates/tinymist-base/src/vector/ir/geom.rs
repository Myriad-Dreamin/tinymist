use core::fmt;
use std::{
    cmp::Ordering,
    hash::{Hash, Hasher},
    sync::Arc,
};

#[cfg(feature = "rkyv")]
use rkyv::{Archive, Deserialize as rDeser, Serialize as rSer};

use super::PathItem;

/// Scalar value of Vector representation.
/// Note: Unlike Typst's Scalar, all lengths with Scalar type are in pt.
#[derive(Default, Clone, Copy)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct Scalar(pub f32);

impl Scalar {
    fn is_zero(&self) -> bool {
        self.0 == 0.0
    }
}

impl From<f32> for Scalar {
    fn from(float: f32) -> Self {
        Self(float)
    }
}

impl From<Scalar> for f32 {
    fn from(scalar: Scalar) -> Self {
        scalar.0
    }
}

impl fmt::Debug for Scalar {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::ops::Add for Scalar {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::Sub for Scalar {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl std::ops::Mul for Scalar {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self(self.0 * rhs.0)
    }
}

impl std::ops::Div for Scalar {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        Self(self.0 / rhs.0)
    }
}

impl Eq for Scalar {}

impl PartialEq for Scalar {
    fn eq(&self, other: &Self) -> bool {
        assert!(!self.0.is_nan() && !other.0.is_nan(), "float is NaN");
        self.0 == other.0
    }
}

impl PartialEq<f32> for Scalar {
    fn eq(&self, other: &f32) -> bool {
        self == &Self(*other)
    }
}

impl Ord for Scalar {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.partial_cmp(&other.0).expect("float is NaN")
    }
}

impl PartialOrd for Scalar {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }

    fn lt(&self, other: &Self) -> bool {
        self.0 < other.0
    }

    fn le(&self, other: &Self) -> bool {
        self.0 <= other.0
    }

    fn gt(&self, other: &Self) -> bool {
        self.0 > other.0
    }

    fn ge(&self, other: &Self) -> bool {
        self.0 >= other.0
    }
}

impl std::ops::Neg for Scalar {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

impl Hash for Scalar {
    fn hash<H: Hasher>(&self, state: &mut H) {
        debug_assert!(!self.0.is_nan(), "float is NaN");
        // instead of bits we swap the bytes on platform with BigEndian
        self.0.to_le_bytes().hash(state);
    }
}

/// length in pt.
pub type Abs = Scalar;
/// Size in (width pt, height pt)
pub type Size = Axes<Abs>;
/// Point in (x pt, y pt)
pub type Point = Axes<Scalar>;
/// Ratio within range [0, 1]
pub type Ratio = Scalar;
/// Angle in radians
pub type Angle = Scalar;

/// A container with a horizontal and vertical component.
#[derive(Default, Debug, Copy, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct Axes<T> {
    /// The horizontal component.
    pub x: T,
    /// The vertical component.
    pub y: T,
}

impl<T> Axes<T> {
    pub fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

// impl Add for Axes
impl<T> std::ops::Add for Axes<T>
where
    T: std::ops::Add<Output = T>,
{
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl From<tiny_skia_path::Point> for Point {
    fn from(typst_axes: tiny_skia_path::Point) -> Self {
        Self {
            x: typst_axes.x.into(),
            y: typst_axes.y.into(),
        }
    }
}

impl From<Point> for tiny_skia_path::Point {
    fn from(axes: Point) -> Self {
        Self {
            x: axes.x.into(),
            y: axes.y.into(),
        }
    }
}

/// A scale-skew-translate transformation.
#[repr(C)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub struct Transform {
    pub sx: Ratio,
    pub ky: Ratio,
    pub kx: Ratio,
    pub sy: Ratio,
    pub tx: Abs,
    pub ty: Abs,
}

impl Transform {
    pub fn from_scale(sx: Ratio, sy: Ratio) -> Self {
        Self {
            sx,
            ky: 0.0.into(),
            kx: 0.0.into(),
            sy,
            tx: 0.0.into(),
            ty: 0.0.into(),
        }
    }

    pub fn from_translate(tx: Abs, ty: Abs) -> Self {
        Self {
            sx: 1.0.into(),
            ky: 0.0.into(),
            kx: 0.0.into(),
            sy: 1.0.into(),
            tx,
            ty,
        }
    }

    pub fn from_skew(kx: Ratio, ky: Ratio) -> Self {
        Self {
            sx: 1.0.into(),
            ky,
            kx,
            sy: 1.0.into(),
            tx: 0.0.into(),
            ty: 0.0.into(),
        }
    }

    pub fn identity() -> Self {
        Self::from_scale(1.0.into(), 1.0.into())
    }

    #[inline]
    pub fn pre_concat(self, other: Self) -> Self {
        let ts: tiny_skia_path::Transform = self.into();
        let other: tiny_skia_path::Transform = other.into();
        let ts = ts.pre_concat(other);
        ts.into()
    }

    #[inline]
    pub fn post_concat(self, other: Self) -> Self {
        other.pre_concat(self)
    }

    #[inline]
    pub fn pre_translate(self, tx: f32, ty: f32) -> Self {
        let ts: tiny_skia_path::Transform = self.into();
        let ts = ts.pre_translate(tx, ty);
        ts.into()
    }

    /// Whether this is the identity transformation.
    pub fn is_identity(self) -> bool {
        self == Self::identity()
    }

    /// Inverts the transformation.
    ///
    /// Returns `None` if the determinant of the matrix is zero.
    pub fn invert(self) -> Option<Self> {
        // Allow the trivial case to be inlined.
        if self.is_identity() {
            return Some(self);
        }

        // Fast path for scale-translate-only transforms.
        if self.kx.is_zero() && self.ky.is_zero() {
            if self.sx.is_zero() || self.sy.is_zero() {
                return Some(Self::from_translate(-self.tx, -self.ty));
            }

            let inv_x = 1.0 / self.sx.0;
            let inv_y = 1.0 / self.sy.0;
            return Some(Self {
                sx: Scalar(inv_x),
                ky: Scalar(0.),
                kx: Scalar(0.),
                sy: Scalar(inv_y),
                tx: Scalar(-self.tx.0 * inv_x),
                ty: Scalar(-self.ty.0 * inv_y),
            });
        }

        let det = self.sx.0 * self.sy.0 - self.kx.0 * self.ky.0;
        if det.abs() < 1e-12 {
            return None;
        }

        let inv_det = 1.0 / det;
        Some(Self {
            sx: Scalar(self.sy.0 * inv_det),
            ky: Scalar(-self.ky.0 * inv_det),
            kx: Scalar(-self.kx.0 * inv_det),
            sy: Scalar(self.sx.0 * inv_det),
            tx: Scalar((self.kx.0 * self.ty.0 - self.sy.0 * self.tx.0) * inv_det),
            ty: Scalar((self.ky.0 * self.tx.0 - self.sx.0 * self.ty.0) * inv_det),
        })
    }
}

impl From<tiny_skia_path::Transform> for Transform {
    fn from(skia_transform: tiny_skia_path::Transform) -> Self {
        Self {
            sx: skia_transform.sx.into(),
            ky: skia_transform.ky.into(),
            kx: skia_transform.kx.into(),
            sy: skia_transform.sy.into(),
            tx: skia_transform.tx.into(),
            ty: skia_transform.ty.into(),
        }
    }
}

impl From<Transform> for tiny_skia_path::Transform {
    fn from(ir_transform: Transform) -> Self {
        Self {
            sx: ir_transform.sx.into(),
            ky: ir_transform.ky.into(),
            kx: ir_transform.kx.into(),
            sy: ir_transform.sy.into(),
            tx: ir_transform.tx.into(),
            ty: ir_transform.ty.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Default)]
pub struct Rect {
    pub lo: Point,
    pub hi: Point,
}

impl Rect {
    pub fn empty() -> Self {
        Self {
            lo: Point::new(Scalar(0.), Scalar(0.)),
            hi: Point::new(Scalar(0.), Scalar(0.)),
        }
    }

    pub fn cano(&self) -> Self {
        let Rect { lo, hi } = self;
        Self {
            lo: Point::new(lo.x.min(hi.x), lo.y.min(hi.y)),
            hi: Point::new(lo.x.max(hi.x), lo.y.max(hi.y)),
        }
    }

    /// Returns whether the rectangle has no area.
    pub fn is_empty(&self) -> bool {
        self.lo.x >= self.hi.x || self.lo.y >= self.hi.y
    }

    /// Returns whether the rectangle is not well constructed.
    ///
    /// Note: This is not the same as `is_empty`.
    pub fn is_intersected(&self) -> bool {
        self.lo.x <= self.hi.x || self.lo.y <= self.hi.y
    }

    pub fn intersect(&self, other: &Self) -> Self {
        Self {
            lo: self.lo.max(&other.lo),
            hi: self.hi.min(&other.hi),
        }
    }

    pub fn union(&self, other: &Self) -> Self {
        if self.is_empty() {
            return *other;
        }

        Self {
            lo: self.lo.min(&other.lo),
            hi: self.hi.max(&other.hi),
        }
    }

    pub fn translate(&self, dp: Point) -> Self {
        Self {
            lo: self.lo + dp,
            hi: self.hi + dp,
        }
    }

    pub fn width(&self) -> Scalar {
        self.hi.x - self.lo.x
    }

    pub fn height(&self) -> Scalar {
        self.hi.y - self.lo.y
    }

    pub fn left(&self) -> Scalar {
        self.lo.x
    }

    pub fn right(&self) -> Scalar {
        self.hi.x
    }

    pub fn top(&self) -> Scalar {
        self.lo.y
    }

    pub fn bottom(&self) -> Scalar {
        self.hi.y
    }
}

impl From<tiny_skia_path::Rect> for Rect {
    fn from(rect: tiny_skia_path::Rect) -> Self {
        Self {
            lo: Point::new(Scalar(rect.left()), Scalar(rect.top())),
            hi: Point::new(Scalar(rect.right()), Scalar(rect.bottom())),
        }
    }
}

impl TryFrom<Rect> for tiny_skia_path::Rect {
    type Error = ();

    fn try_from(rect: Rect) -> Result<Self, Self::Error> {
        Self::from_ltrb(rect.lo.x.0, rect.lo.y.0, rect.hi.x.0, rect.hi.y.0).ok_or(())
    }
}

pub trait EuclidMinMax {
    fn min(&self, other: &Self) -> Self;
    fn max(&self, other: &Self) -> Self;
}

impl EuclidMinMax for Scalar {
    fn min(&self, other: &Self) -> Self {
        Self(self.0.min(other.0))
    }

    fn max(&self, other: &Self) -> Self {
        Self(self.0.max(other.0))
    }
}

impl EuclidMinMax for Point {
    fn min(&self, other: &Self) -> Self {
        Self {
            x: self.x.min(other.x),
            y: self.y.min(other.y),
        }
    }

    fn max(&self, other: &Self) -> Self {
        Self {
            x: self.x.max(other.x),
            y: self.y.max(other.y),
        }
    }
}

/// Item representing all the transform that is applicable to a
/// [`super::VecItem`]. See <https://developer.mozilla.org/en-US/docs/Web/SVG/Attribute/transform>
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[cfg_attr(feature = "rkyv", derive(Archive, rDeser, rSer))]
#[cfg_attr(feature = "rkyv-validation", archive(check_bytes))]
pub enum TransformItem {
    /// `matrix` transform.
    Matrix(Arc<Transform>),
    /// `translate` transform.
    Translate(Arc<Axes<Abs>>),
    /// `scale` transform.
    Scale(Arc<(Ratio, Ratio)>),
    /// `rotate` transform.
    Rotate(Arc<Scalar>),
    /// `skewX skewY` transform.
    Skew(Arc<(Ratio, Ratio)>),

    /// clip path.
    Clip(Arc<PathItem>),
}

/// See [`TransformItem`].
impl From<TransformItem> for Transform {
    fn from(value: TransformItem) -> Self {
        match value {
            TransformItem::Matrix(m) => *m,
            TransformItem::Scale(m) => Transform::from_scale(m.0, m.1),
            TransformItem::Translate(m) => Transform::from_translate(m.x, m.y),
            TransformItem::Rotate(_m) => todo!(),
            TransformItem::Skew(m) => Transform::from_skew(m.0, m.1),
            TransformItem::Clip(_m) => Transform::identity(),
        }
    }
}
