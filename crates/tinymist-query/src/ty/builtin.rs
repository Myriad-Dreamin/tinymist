use core::fmt;
use std::hash::{Hash, Hasher};

use ecow::{eco_format, EcoString};
use once_cell::sync::Lazy;
use regex::RegexSet;
use strum::{EnumIter, IntoEnumIterator};
use typst::foundations::CastInfo;
use typst::layout::{Fr, Rel};
use typst::{
    foundations::{
        Args as TypstArgs, AutoValue, Bytes, Content as TypstContent, Datetime as TypstDatetime,
        Decimal, Duration as TypstDuration, Dynamic, Func, Label, Module as TypstModule, NoneValue,
        ParamInfo, Plugin as TypstPlugin, Str, Styles as TypstStyles, Type, Value, Version,
    },
    layout::{Angle as TypstAngle, Length as TypstLength, Ratio as TypstRatio},
    symbols::Symbol as TypstSymbol,
    visualize::{Color as TypstColor, Gradient as TypstGradient, Pattern as TypstPattern},
};

use crate::syntax::Decl;
use crate::ty::*;

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, EnumIter)]
pub enum PathPreference {
    Source { allow_package: bool },
    Csv,
    Image,
    Json,
    Yaml,
    Xml,
    Toml,
    Csl,
    Bibliography,
    RawTheme,
    RawSyntax,
    Special,
    None,
}

impl PathPreference {
    pub fn ext_matcher(&self) -> &'static RegexSet {
        static SOURCE_REGSET: Lazy<RegexSet> =
            Lazy::new(|| RegexSet::new([r"^typ$", r"^typc$"]).unwrap());
        static IMAGE_REGSET: Lazy<RegexSet> = Lazy::new(|| {
            RegexSet::new([
                r"^ico$", r"^bmp$", r"^png$", r"^webp$", r"^jpg$", r"^jpeg$", r"^jfif$", r"^tiff$",
                r"^gif$", r"^svg$", r"^svgz$",
            ])
            .unwrap()
        });
        static JSON_REGSET: Lazy<RegexSet> =
            Lazy::new(|| RegexSet::new([r"^json$", r"^jsonc$", r"^json5$"]).unwrap());
        static YAML_REGSET: Lazy<RegexSet> =
            Lazy::new(|| RegexSet::new([r"^yaml$", r"^yml$"]).unwrap());
        static XML_REGSET: Lazy<RegexSet> = Lazy::new(|| RegexSet::new([r"^xml$"]).unwrap());
        static TOML_REGSET: Lazy<RegexSet> = Lazy::new(|| RegexSet::new([r"^toml$"]).unwrap());
        static CSV_REGSET: Lazy<RegexSet> = Lazy::new(|| RegexSet::new([r"^csv$"]).unwrap());
        static BIB_REGSET: Lazy<RegexSet> =
            Lazy::new(|| RegexSet::new([r"^yaml$", r"^yml$", r"^bib$"]).unwrap());
        static CSL_REGSET: Lazy<RegexSet> = Lazy::new(|| RegexSet::new([r"^csl$"]).unwrap());
        static RAW_THEME_REGSET: Lazy<RegexSet> =
            Lazy::new(|| RegexSet::new([r"^tmTheme$", r"^xml$"]).unwrap());
        static RAW_SYNTAX_REGSET: Lazy<RegexSet> =
            Lazy::new(|| RegexSet::new([r"^tmLanguage$", r"^sublime-syntax$"]).unwrap());
        static ALL_REGSET: Lazy<RegexSet> = Lazy::new(|| RegexSet::new([r".*"]).unwrap());
        static ALL_SPECIAL_REGSET: Lazy<RegexSet> = Lazy::new(|| {
            RegexSet::new({
                let patterns = SOURCE_REGSET.patterns();
                let patterns = patterns.iter().chain(IMAGE_REGSET.patterns());
                let patterns = patterns.chain(JSON_REGSET.patterns());
                let patterns = patterns.chain(YAML_REGSET.patterns());
                let patterns = patterns.chain(XML_REGSET.patterns());
                let patterns = patterns.chain(TOML_REGSET.patterns());
                let patterns = patterns.chain(CSV_REGSET.patterns());
                let patterns = patterns.chain(BIB_REGSET.patterns());
                let patterns = patterns.chain(CSL_REGSET.patterns());
                let patterns = patterns.chain(RAW_THEME_REGSET.patterns());
                patterns.chain(RAW_SYNTAX_REGSET.patterns())
            })
            .unwrap()
        });

        match self {
            PathPreference::Source { .. } => &SOURCE_REGSET,
            PathPreference::Csv => &CSV_REGSET,
            PathPreference::Image => &IMAGE_REGSET,
            PathPreference::Json => &JSON_REGSET,
            PathPreference::Yaml => &YAML_REGSET,
            PathPreference::Xml => &XML_REGSET,
            PathPreference::Toml => &TOML_REGSET,
            PathPreference::Csl => &CSL_REGSET,
            PathPreference::Bibliography => &BIB_REGSET,
            PathPreference::RawTheme => &RAW_THEME_REGSET,
            PathPreference::RawSyntax => &RAW_SYNTAX_REGSET,
            PathPreference::Special => &ALL_SPECIAL_REGSET,
            PathPreference::None => &ALL_REGSET,
        }
    }

    pub fn from_ext(path: &str) -> Option<Self> {
        let path = std::path::Path::new(path).extension()?.to_str()?;
        PathPreference::iter().find(|preference| preference.ext_matcher().is_match(path))
    }
}

impl Ty {
    pub(crate) fn from_cast_info(ty: &CastInfo) -> Ty {
        match &ty {
            CastInfo::Any => Ty::Any,
            CastInfo::Value(val, doc) => Ty::Value(InsTy::new_doc(val.clone(), *doc)),
            CastInfo::Type(ty) => Ty::Lit(LitTy::Type(*ty)),
            CastInfo::Union(types) => {
                Ty::iter_union(UnionIter(vec![types.as_slice().iter()]).map(Self::from_cast_info))
            }
        }
    }

    pub(crate) fn from_param_site(func: &Func, param: &ParamInfo) -> Ty {
        use typst::foundations::func::Repr;
        match func.inner() {
            Repr::Element(..) | Repr::Native(..) => {
                if let Some(ty) = param_mapping(func, param) {
                    return ty;
                }
            }
            Repr::Closure(_) => {}
            Repr::With(w) => return Ty::from_param_site(&w.0, param),
        };

        Self::from_cast_info(&param.input)
    }

    pub(crate) fn from_return_site(func: &Func, ty: &'_ CastInfo) -> Self {
        use typst::foundations::func::Repr;
        match func.inner() {
            Repr::Element(elem) => return Ty::Lit(LitTy::Element(*elem)),
            Repr::Closure(_) => {}
            Repr::With(w) => return Ty::from_return_site(&w.0, ty),
            Repr::Native(_) => {}
        };

        Self::from_cast_info(ty)
    }
}

struct UnionIter<'a>(Vec<std::slice::Iter<'a, CastInfo>>);

impl<'a> Iterator for UnionIter<'a> {
    type Item = &'a CastInfo;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let iter = self.0.last_mut()?;
            if let Some(ty) = iter.next() {
                match ty {
                    CastInfo::Union(types) => {
                        self.0.push(types.as_slice().iter());
                    }
                    _ => return Some(ty),
                }
            } else {
                self.0.pop();
            }
        }
    }
}

// todo: we can write some proto files for builtin sigs
#[derive(Debug, Clone, Copy)]
pub enum BuiltinSig<'a> {
    /// Map a function over a tuple.
    TupleMap(&'a Ty),
    /// Get element of a tuple.
    TupleAt(&'a Ty),
}

/// A package identifier.
#[derive(Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackageId {
    pub namespace: StrRef,
    pub name: StrRef,
}

impl fmt::Debug for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}/{}", self.namespace, self.name)
    }
}

impl TryFrom<TypstFileId> for PackageId {
    type Error = ();

    fn try_from(value: TypstFileId) -> Result<Self, Self::Error> {
        let Some(spec) = value.package() else {
            return Err(());
        };
        Ok(PackageId {
            namespace: spec.namespace.as_str().into(),
            name: spec.name.as_str().into(),
        })
    }
}

#[derive(Clone)]
pub enum LitTy {
    Clause,
    Undef,
    Space,
    /// The value that indicates the absence of a meaningful value.
    None,
    Break,
    Continue,
    Infer,
    FlowNone,
    /// A value that indicates some smart default behavior.
    Auto,

    TextSize,
    TextFont,
    TextLang,
    TextRegion,

    /// A label: `<intro>`.
    Label(Option<Label>),
    CiteLabel,
    RefLabel,
    Dir,

    Stroke,
    Margin,
    Inset,
    Outset,
    Radius,

    Tag(Box<(StrRef, Option<Interned<PackageId>>)>),
    Type(typst::foundations::Type),
    TypeType(typst::foundations::Type),
    Element(typst::foundations::Element),
    Module(Interned<Decl>),
    Path(PathPreference),

    /// An integer: `120`.
    Int(Option<i64>),
    /// A floating-point number: `1.2`, `10e-4`.
    Float(Option<f64>),
    /// A length: `12pt`, `3cm`, `1.5em`, `1em - 2pt`.
    Length(Option<TypstLength>),
    /// An angle: `1.5rad`, `90deg`.
    Angle(Option<TypstAngle>),
    /// A ratio: `50%`.
    Ratio(Option<TypstRatio>),
    /// A relative length, combination of a ratio and a length: `20% + 5cm`.
    Relative(Option<Rel<TypstLength>>),
    /// A fraction: `1fr`.
    Fraction(Option<Fr>),
    /// A color value: `#f79143ff`.
    Color(Option<TypstColor>),
    /// A gradient value: `gradient.linear(...)`.
    Gradient(Option<TypstGradient>),
    /// A pattern fill: `pattern(...)`.
    Pattern(Option<TypstPattern>),
    /// A symbol: `arrow.l`.
    Symbol(Option<TypstSymbol>),
    /// A version.
    Version(Option<Version>),
    /// A string: `"string"`.
    Str(Option<Str>),
    /// Raw bytes.
    Bytes(Option<Bytes>),
    /// A datetime
    Datetime(Option<TypstDatetime>),
    /// A decimal value: `decimal("123.4500")`
    Decimal(Option<Decimal>),
    /// A duration
    Duration(Option<TypstDuration>),
    /// A content value: `[*Hi* there]`.
    Content(Option<TypstContent>),
    /// Content styles.
    Styles(Option<TypstStyles>),
    /// Captured arguments to a function.
    Args(Option<TypstArgs>),
    /// A module.
    BuiltinModule(TypstModule),
    /// A WebAssembly plugin.
    Plugin(TypstPlugin),
    /// A dynamic value.
    Dyn(Dynamic),
}

impl Hash for LitTy {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Clause => {}
            Self::Undef => {}
            Self::Space => {}
            Self::None => {}
            Self::Break => {}
            Self::Continue => {}
            Self::Infer => {}
            Self::FlowNone => {}
            Self::Auto => {}

            Self::TextSize => {}
            Self::TextFont => {}
            Self::TextLang => {}
            Self::TextRegion => {}

            Self::Label(v) => v.hash(state),
            Self::CiteLabel => {}
            Self::RefLabel => {}
            Self::Dir => {}

            Self::Stroke => {}
            Self::Margin => {}
            Self::Inset => {}
            Self::Outset => {}
            Self::Radius => {}

            Self::Tag(v) => v.hash(state),
            Self::Type(v) => v.hash(state),
            Self::TypeType(v) => v.hash(state),
            Self::Element(v) => v.hash(state),
            Self::Module(v) => v.hash(state),
            Self::Path(v) => v.hash(state),

            Self::Int(v) => v.hash(state),
            Self::Float(Some(v)) => v.to_bits().hash(state),
            Self::Float(Option::None) => 0u8.hash(state),
            Self::Length(v) => v.hash(state),
            Self::Angle(v) => v.hash(state),
            Self::Ratio(v) => v.hash(state),
            Self::Relative(v) => v.hash(state),
            Self::Fraction(v) => v.hash(state),
            Self::Color(v) => v.hash(state),
            Self::Gradient(v) => v.hash(state),
            Self::Pattern(v) => v.hash(state),
            Self::Symbol(v) => v.hash(state),
            Self::Version(v) => v.hash(state),
            Self::Str(v) => v.hash(state),
            Self::Bytes(v) => v.hash(state),
            Self::Datetime(v) => v.hash(state),
            Self::Decimal(v) => v.hash(state),
            Self::Duration(v) => v.hash(state),
            Self::Content(v) => v.hash(state),
            Self::Styles(v) => v.hash(state),
            Self::Args(v) => v.hash(state),
            Self::BuiltinModule(v) => v.hash(state),
            Self::Plugin(v) => v.hash(state),
            Self::Dyn(v) => v.hash(state),
        }
    }
}

impl PartialEq for LitTy {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
            && match (self, other) {
                (Self::Clause, Self::Clause) => true,
                (Self::Undef, Self::Undef) => true,
                (Self::Space, Self::Space) => true,
                (Self::None, Self::None) => true,
                (Self::Break, Self::Break) => true,
                (Self::Continue, Self::Continue) => true,
                (Self::Infer, Self::Infer) => true,
                (Self::FlowNone, Self::FlowNone) => true,
                (Self::Auto, Self::Auto) => true,

                (Self::TextSize, Self::TextSize) => true,
                (Self::TextFont, Self::TextFont) => true,
                (Self::TextLang, Self::TextLang) => true,
                (Self::TextRegion, Self::TextRegion) => true,

                (Self::Label(v1), Self::Label(v2)) => v1 == v2,
                (Self::CiteLabel, Self::CiteLabel) => true,
                (Self::RefLabel, Self::RefLabel) => true,
                (Self::Dir, Self::Dir) => true,

                (Self::Stroke, Self::Stroke) => true,
                (Self::Margin, Self::Margin) => true,
                (Self::Inset, Self::Inset) => true,
                (Self::Outset, Self::Outset) => true,
                (Self::Radius, Self::Radius) => true,

                (Self::Tag(v1), Self::Tag(v2)) => v1 == v2,
                (Self::Type(v1), Self::Type(v2)) => v1 == v2,
                (Self::TypeType(v1), Self::TypeType(v2)) => v1 == v2,
                (Self::Element(v1), Self::Element(v2)) => v1 == v2,
                (Self::Module(v1), Self::Module(v2)) => v1 == v2,
                (Self::Path(v1), Self::Path(v2)) => v1 == v2,

                (Self::Int(v1), Self::Int(v2)) => v1 == v2,
                (Self::Float(Some(v1)), Self::Float(Some(v2))) => v1.to_bits() == v2.to_bits(),
                (Self::Float(Option::None), Self::Float(Option::None)) => true,
                (Self::Length(v1), Self::Length(v2)) => v1 == v2,
                (Self::Angle(v1), Self::Angle(v2)) => v1 == v2,
                (Self::Ratio(v1), Self::Ratio(v2)) => v1 == v2,
                (Self::Relative(v1), Self::Relative(v2)) => v1 == v2,
                (Self::Fraction(v1), Self::Fraction(v2)) => v1 == v2,
                (Self::Color(v1), Self::Color(v2)) => v1 == v2,
                (Self::Gradient(v1), Self::Gradient(v2)) => v1 == v2,
                (Self::Pattern(v1), Self::Pattern(v2)) => v1 == v2,
                (Self::Symbol(v1), Self::Symbol(v2)) => v1 == v2,
                (Self::Version(v1), Self::Version(v2)) => v1 == v2,
                (Self::Str(v1), Self::Str(v2)) => v1 == v2,
                (Self::Bytes(v1), Self::Bytes(v2)) => v1 == v2,
                (Self::Datetime(v1), Self::Datetime(v2)) => v1 == v2,
                (Self::Decimal(v1), Self::Decimal(v2)) => v1 == v2,
                (Self::Duration(v1), Self::Duration(v2)) => v1 == v2,
                (Self::Content(v1), Self::Content(v2)) => v1 == v2,
                (Self::Styles(v1), Self::Styles(v2)) => v1 == v2,
                (Self::Args(v1), Self::Args(v2)) => v1 == v2,
                (Self::BuiltinModule(v1), Self::BuiltinModule(v2)) => v1 == v2,
                (Self::Plugin(v1), Self::Plugin(v2)) => v1 == v2,
                (Self::Dyn(v1), Self::Dyn(v2)) => v1 == v2,
                _ => false,
            }
    }
}

// Hash, PartialEq, PartialOrd, Ord

// , PartialEq, Eq, PartialOrd, Ord, Hash)]

impl Eq for LitTy {}

impl PartialOrd for LitTy {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LitTy {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (Self::Clause, Self::Clause) => std::cmp::Ordering::Equal,
            (Self::Undef, Self::Undef) => std::cmp::Ordering::Equal,
            (Self::Space, Self::Space) => std::cmp::Ordering::Equal,
            (Self::None, Self::None) => std::cmp::Ordering::Equal,
            (Self::Break, Self::Break) => std::cmp::Ordering::Equal,
            (Self::Continue, Self::Continue) => std::cmp::Ordering::Equal,
            (Self::Infer, Self::Infer) => std::cmp::Ordering::Equal,
            (Self::FlowNone, Self::FlowNone) => std::cmp::Ordering::Equal,
            (Self::Auto, Self::Auto) => std::cmp::Ordering::Equal,

            (Self::TextSize, Self::TextSize) => std::cmp::Ordering::Equal,
            (Self::TextFont, Self::TextFont) => std::cmp::Ordering::Equal,
            (Self::TextLang, Self::TextLang) => std::cmp::Ordering::Equal,
            (Self::TextRegion, Self::TextRegion) => std::cmp::Ordering::Equal,

            (Self::Label(v1), Self::Label(v2)) => v1.cmp(v2),
            (Self::CiteLabel, Self::CiteLabel) => std::cmp::Ordering::Equal,
            (Self::RefLabel, Self::RefLabel) => std::cmp::Ordering::Equal,
            (Self::Dir, Self::Dir) => std::cmp::Ordering::Equal,

            (Self::Stroke, Self::Stroke) => std::cmp::Ordering::Equal,
            (Self::Margin, Self::Margin) => std::cmp::Ordering::Equal,
            (Self::Inset, Self::Inset) => std::cmp::Ordering::Equal,
            (Self::Outset, Self::Outset) => std::cmp::Ordering::Equal,
            (Self::Radius, Self::Radius) => std::cmp::Ordering::Equal,

            (Self::Tag(v1), Self::Tag(v2)) => v1.cmp(v2),
            (Self::Type(v1), Self::Type(v2)) => v1.cmp(v2),
            (Self::TypeType(v1), Self::TypeType(v2)) => v1.cmp(v2),
            (Self::Element(v1), Self::Element(v2)) => v1.cmp(v2),
            (Self::Module(v1), Self::Module(v2)) => v1.cmp(v2),
            (Self::Path(v1), Self::Path(v2)) => v1.cmp(v2),

            (Self::Int(v1), Self::Int(v2)) => v1.cmp(v2),
            (Self::Float(Some(v1)), Self::Float(Some(v2))) => v1.to_bits().cmp(&v2.to_bits()),
            (Self::Float(Option::None), Self::Float(Option::None)) => std::cmp::Ordering::Equal,
            (Self::Float(Some(_)), Self::Float(Option::None)) => std::cmp::Ordering::Greater,
            (Self::Float(Option::None), Self::Float(Some(_))) => std::cmp::Ordering::Less,
            (Self::Length(v1), Self::Length(v2)) => {
                v1.partial_cmp(v2).unwrap_or(std::cmp::Ordering::Equal)
            }
            (Self::Angle(v1), Self::Angle(v2)) => v1.cmp(v2),
            (Self::Ratio(v1), Self::Ratio(v2)) => v1.cmp(v2),
            (Self::Relative(v1), Self::Relative(v2)) => {
                v1.partial_cmp(v2).unwrap_or(std::cmp::Ordering::Equal)
            }
            (Self::Fraction(v1), Self::Fraction(v2)) => v1.cmp(v2),
            (Self::Color(v1), Self::Color(v2)) => hash_cmp(v1, v2),
            (Self::Gradient(v1), Self::Gradient(v2)) => hash_cmp(v1, v2),
            (Self::Pattern(v1), Self::Pattern(v2)) => hash_cmp(v1, v2),
            (Self::Symbol(v1), Self::Symbol(v2)) => hash_cmp(v1, v2),
            (Self::Version(v1), Self::Version(v2)) => v1.cmp(v2),
            (Self::Str(v1), Self::Str(v2)) => v1.cmp(v2),
            (Self::Bytes(v1), Self::Bytes(v2)) => hash_cmp(v1, v2),
            (Self::Datetime(v1), Self::Datetime(v2)) => {
                v1.partial_cmp(v2).unwrap_or(std::cmp::Ordering::Equal)
            }
            (Self::Decimal(v1), Self::Decimal(v2)) => v1.cmp(v2),
            (Self::Duration(v1), Self::Duration(v2)) => v1.cmp(v2),
            (Self::Content(v1), Self::Content(v2)) => hash_cmp(v1, v2),
            (Self::Styles(v1), Self::Styles(v2)) => hash_cmp(v1, v2),
            (Self::Args(v1), Self::Args(v2)) => hash_cmp(v1, v2),
            (Self::BuiltinModule(v1), Self::BuiltinModule(v2)) => hash_cmp(v1, v2),
            (Self::Plugin(v1), Self::Plugin(v2)) => hash_cmp(v1, v2),
            (Self::Dyn(v1), Self::Dyn(v2)) => hash_cmp(v1, v2),

            (Self::Clause, _) => std::cmp::Ordering::Less,
            (Self::Undef, _) => std::cmp::Ordering::Less,
            (Self::Space, _) => std::cmp::Ordering::Less,
            (Self::None, _) => std::cmp::Ordering::Less,
            (Self::Break, _) => std::cmp::Ordering::Less,
            (Self::Continue, _) => std::cmp::Ordering::Less,
            (Self::Infer, _) => std::cmp::Ordering::Less,
            (Self::FlowNone, _) => std::cmp::Ordering::Less,
            (Self::Auto, _) => std::cmp::Ordering::Less,

            (Self::TextSize, _) => std::cmp::Ordering::Less,
            (Self::TextFont, _) => std::cmp::Ordering::Less,
            (Self::TextLang, _) => std::cmp::Ordering::Less,
            (Self::TextRegion, _) => std::cmp::Ordering::Less,

            (Self::Label(..), _) => std::cmp::Ordering::Less,
            (Self::CiteLabel, _) => std::cmp::Ordering::Less,
            (Self::RefLabel, _) => std::cmp::Ordering::Less,
            (Self::Dir, _) => std::cmp::Ordering::Less,

            (Self::Stroke, _) => std::cmp::Ordering::Less,
            (Self::Margin, _) => std::cmp::Ordering::Less,
            (Self::Inset, _) => std::cmp::Ordering::Less,
            (Self::Outset, _) => std::cmp::Ordering::Less,
            (Self::Radius, _) => std::cmp::Ordering::Less,

            (Self::Tag(..), _) => std::cmp::Ordering::Less,
            (Self::Type(..), _) => std::cmp::Ordering::Less,
            (Self::TypeType(..), _) => std::cmp::Ordering::Less,
            (Self::Element(..), _) => std::cmp::Ordering::Less,
            (Self::Module(..), _) => std::cmp::Ordering::Less,
            (Self::Path(..), _) => std::cmp::Ordering::Less,

            (Self::Int(..), _) => std::cmp::Ordering::Less,
            (Self::Float(..), _) => std::cmp::Ordering::Less,
            (Self::Length(..), _) => std::cmp::Ordering::Less,
            (Self::Angle(..), _) => std::cmp::Ordering::Less,
            (Self::Ratio(..), _) => std::cmp::Ordering::Less,
            (Self::Relative(..), _) => std::cmp::Ordering::Less,
            (Self::Fraction(..), _) => std::cmp::Ordering::Less,
            (Self::Color(..), _) => std::cmp::Ordering::Less,
            (Self::Gradient(..), _) => std::cmp::Ordering::Less,
            (Self::Pattern(..), _) => std::cmp::Ordering::Less,
            (Self::Symbol(..), _) => std::cmp::Ordering::Less,
            (Self::Version(..), _) => std::cmp::Ordering::Less,
            (Self::Str(..), _) => std::cmp::Ordering::Less,
            (Self::Bytes(..), _) => std::cmp::Ordering::Less,
            (Self::Datetime(..), _) => std::cmp::Ordering::Less,
            (Self::Decimal(..), _) => std::cmp::Ordering::Less,
            (Self::Duration(..), _) => std::cmp::Ordering::Less,
            (Self::Content(..), _) => std::cmp::Ordering::Less,
            (Self::Styles(..), _) => std::cmp::Ordering::Less,
            (Self::Args(..), _) => std::cmp::Ordering::Less,
            (Self::BuiltinModule(..), _) => std::cmp::Ordering::Less,
            (Self::Plugin(..), _) => std::cmp::Ordering::Less,

            (Self::Dyn(..), _) => std::cmp::Ordering::Greater,
        }
    }
}

fn hash_cmp<T: Hash>(v1: &T, v2: &T) -> std::cmp::Ordering {
    reflexo_typst::hash::hash128(v1).cmp(&reflexo_typst::hash::hash128(v2))
}

impl fmt::Debug for LitTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LitTy::Clause => f.write_str("Clause"),
            LitTy::Undef => f.write_str("Undef"),
            LitTy::Content(Option::None) => f.write_str("Content"),
            LitTy::Space => f.write_str("Space"),
            LitTy::None => f.write_str("None"),
            LitTy::Break => f.write_str("Break"),
            LitTy::Continue => f.write_str("Continue"),
            LitTy::Infer => f.write_str("Infer"),
            LitTy::FlowNone => f.write_str("FlowNone"),
            LitTy::Auto => f.write_str("Auto"),

            LitTy::Int(Option::None) => write!(f, "Integer"),
            LitTy::Datetime(Option::None) => write!(f, "Datetime"),
            LitTy::Args(Option::None) => write!(f, "Args"),
            LitTy::Color(Option::None) => write!(f, "Color"),
            LitTy::TextSize => write!(f, "TextSize"),
            LitTy::TextFont => write!(f, "TextFont"),
            LitTy::TextLang => write!(f, "TextLang"),
            LitTy::TextRegion => write!(f, "TextRegion"),
            LitTy::Dir => write!(f, "Dir"),
            LitTy::Length(Option::None) => write!(f, "Length"),
            LitTy::Label(Option::None) => write!(f, "Label"),
            LitTy::CiteLabel => write!(f, "CiteLabel"),
            LitTy::RefLabel => write!(f, "RefLabel"),
            LitTy::Float(Option::None) => write!(f, "Float"),
            LitTy::Stroke => write!(f, "Stroke"),
            LitTy::Margin => write!(f, "Margin"),
            LitTy::Inset => write!(f, "Inset"),
            LitTy::Outset => write!(f, "Outset"),
            LitTy::Radius => write!(f, "Radius"),
            LitTy::TypeType(ty) => write!(f, "TypeType({})", ty.short_name()),
            LitTy::Type(ty) => write!(f, "Type({})", ty.short_name()),
            LitTy::Element(elem) => elem.fmt(f),
            LitTy::Tag(tag) => {
                let (name, id) = tag.as_ref();
                if let Some(id) = id {
                    write!(f, "Tag({name:?}) of {id:?}")
                } else {
                    write!(f, "Tag({name:?})")
                }
            }
            LitTy::Module(decl) => write!(f, "{decl:?}"),
            LitTy::Path(preference) => write!(f, "Path({preference:?})"),

            Int(Some(v)) => write!(f, "Int({v:?})"),
            Float(Some(v)) => write!(f, "Float({v:?})"),
            Length(Some(v)) => write!(f, "Length({v:?})"),
            Angle(v) => write!(f, "Angle({v:?})"),
            Ratio(v) => write!(f, "Ratio({v:?})"),
            Relative(v) => write!(f, "Relative({v:?})"),
            Fraction(v) => write!(f, "Fraction({v:?})"),
            Color(Some(v)) => write!(f, "Color({v:?})"),
            Gradient(v) => write!(f, "Gradient({v:?})"),
            Pattern(v) => write!(f, "Pattern({v:?})"),
            Symbol(v) => write!(f, "Symbol({v:?})"),
            LitTy::Label(Some(v)) => write!(f, "Label({v:?})"),
            LitTy::Version(v) => write!(f, "Version({v:?})"),
            LitTy::Str(v) => write!(f, "Str({v:?})"),
            LitTy::Bytes(v) => write!(f, "Bytes({v:?})"),
            Datetime(Some(v)) => write!(f, "Datetime({v:?})"),
            LitTy::Decimal(v) => write!(f, "Decimal({v:?})"),
            Duration(v) => write!(f, "Duration({v:?})"),
            Content(Some(v)) => write!(f, "Content({v:?})"),
            Styles(v) => write!(f, "Styles({v:?})"),
            Args(Some(v)) => write!(f, "Args({v:?})"),
            BuiltinModule(v) => write!(f, "BuiltinModule({v:?})"),
            Plugin(v) => write!(f, "Plugin({v:?})"),
            Dyn(v) => write!(f, "Dyn({v:?})"),
        }
    }
}

impl LitTy {
    pub fn from_value(builtin: &Value) -> Ty {
        if let Value::Bool(v) = builtin {
            return Ty::Boolean(Some(*v));
        }

        Self::from_builtin(builtin.ty())
    }

    pub fn from_builtin(builtin: Type) -> Ty {
        if builtin == Type::of::<AutoValue>() {
            return Ty::Lit(LitTy::Auto);
        }
        if builtin == Type::of::<NoneValue>() {
            return Ty::Lit(LitTy::None);
        }
        if builtin == Type::of::<typst::visualize::Color>() {
            return Color(Option::None).literally();
        }
        if builtin == Type::of::<bool>() {
            return Ty::Lit(LitTy::None);
        }
        if builtin == Type::of::<f64>() {
            return Float(Option::None).literally();
        }
        if builtin == Type::of::<TypstLength>() {
            return Length(Option::None).literally();
        }
        if builtin == Type::of::<TypstContent>() {
            return Ty::Lit(LitTy::Content(Option::None));
        }

        LitTy::Type(builtin).literally()
    }

    pub(crate) fn describe(&self) -> EcoString {
        let res = match self {
            LitTy::Clause => "any",
            LitTy::Undef => "any",
            LitTy::Content(Option::None) => "content",
            LitTy::Space => "content",
            LitTy::None => "none",
            LitTy::Break => "break",
            LitTy::Continue => "continue",
            LitTy::Infer => "any",
            LitTy::FlowNone => "none",
            LitTy::Auto => "auto",

            LitTy::Int(Option::None) => "int",
            LitTy::Datetime(Option::None) => "datetime",
            LitTy::Args(Option::None) => "arguments",
            LitTy::Color(Option::None) => "color",
            LitTy::TextSize => "text.size",
            LitTy::TextFont => "text.font",
            LitTy::TextLang => "text.lang",
            LitTy::TextRegion => "text.region",
            LitTy::Dir => "dir",
            LitTy::Length(Option::None) => "length",
            LitTy::Float(Option::None) => "float",
            LitTy::Label(Option::None) => "label",
            LitTy::CiteLabel => "cite-label",
            LitTy::RefLabel => "ref-label",
            LitTy::Stroke => "stroke",
            LitTy::Margin => "margin",
            LitTy::Inset => "inset",
            LitTy::Outset => "outset",
            LitTy::Radius => "radius",
            LitTy::TypeType(..) => "type",
            LitTy::Type(ty) => ty.short_name(),
            LitTy::Element(ty) => ty.name(),
            LitTy::Tag(tag) => {
                let (name, id) = tag.as_ref();
                return if let Some(id) = id {
                    eco_format!("tag {name} of {id:?}")
                } else {
                    eco_format!("tag {name}")
                };
            }
            LitTy::Module(m) => return eco_format!("module({})", m.name()),
            LitTy::Path(s) => match s {
                PathPreference::None => "[any]",
                PathPreference::Special => "[any]",
                PathPreference::Source { .. } => "[source]",
                PathPreference::Csv => "[csv]",
                PathPreference::Image => "[image]",
                PathPreference::Json => "[json]",
                PathPreference::Yaml => "[yaml]",
                PathPreference::Xml => "[xml]",
                PathPreference::Toml => "[toml]",
                PathPreference::Csl => "[csl]",
                PathPreference::Bibliography => "[bib]",
                PathPreference::RawTheme => "[theme]",
                PathPreference::RawSyntax => "[syntax]",
            },

            Int(Some(v)) => return eco_format!("Int({v:?})"),
            Float(Some(v)) => return eco_format!("Float({v:?})"),
            Length(Some(v)) => return eco_format!("Length({v:?})"),
            Angle(v) => return eco_format!("Angle({v:?})"),
            Ratio(v) => return eco_format!("Ratio({v:?})"),
            Relative(v) => return eco_format!("Relative({v:?})"),
            Fraction(v) => return eco_format!("Fraction({v:?})"),
            Color(Some(v)) => return eco_format!("Color({v:?})"),
            Gradient(v) => return eco_format!("Gradient({v:?})"),
            Pattern(v) => return eco_format!("Pattern({v:?})"),
            Symbol(v) => return eco_format!("Symbol({v:?})"),
            LitTy::Label(v) => return eco_format!("Label({v:?})"),
            LitTy::Version(v) => return eco_format!("Version({v:?})"),
            LitTy::Str(v) => return eco_format!("Str({v:?})"),
            LitTy::Bytes(v) => return eco_format!("Bytes({v:?})"),
            Datetime(Some(v)) => return eco_format!("Datetime({v:?})"),
            LitTy::Decimal(v) => return eco_format!("Decimal({v:?})"),
            Duration(v) => return eco_format!("Duration({v:?})"),
            Content(v) => return eco_format!("Content({v:?})"),
            Styles(v) => return eco_format!("Styles({v:?})"),
            Args(v) => return eco_format!("Args({v:?})"),
            BuiltinModule(v) => return eco_format!("BuiltinModule({v:?})"),
            Plugin(v) => return eco_format!("Plugin({v:?})"),
            Dyn(v) => return eco_format!("Dyn({v:?})"),
        };

        res.into()
    }
}

use LitTy::*;

fn literally(s: impl FlowBuiltinLiterally) -> Ty {
    s.literally()
}

trait FlowBuiltinLiterally {
    fn literally(self) -> Ty;
}

impl FlowBuiltinLiterally for &str {
    fn literally(self) -> Ty {
        Ty::Value(InsTy::new(Value::Str(self.into())))
    }
}

impl FlowBuiltinLiterally for LitTy {
    fn literally(self) -> Ty {
        Ty::Lit(self.clone())
    }
}

impl FlowBuiltinLiterally for Ty {
    fn literally(self) -> Ty {
        self
    }
}

// separate by middle
macro_rules! flow_builtin_union_inner {
    ($literal_kind:expr) => {
        literally($literal_kind)
    };
    ($($x:expr),+ $(,)?) => {
        Vec::from_iter([
            $(flow_builtin_union_inner!($x)),*
        ])
    };
}

macro_rules! flow_union {
    // the first one is string
    ($($b:tt)*) => {
        Ty::iter_union(flow_builtin_union_inner!( $($b)* ).into_iter())
    };

}

macro_rules! flow_record {
    ($($name:expr => $ty:expr),* $(,)?) => {
        RecordTy::new(vec![
            $(
                (
                    $name.into(),
                    $ty,
                ),
            )*
        ])
    };
}

pub(super) fn param_mapping(func: &Func, param: &ParamInfo) -> Option<Ty> {
    match (func.name()?, param.name) {
        ("cbor", "path") => Some(literally(Path(PathPreference::None))),
        ("csv", "path") => Some(literally(Path(PathPreference::Csv))),
        ("image", "path") => Some(literally(Path(PathPreference::Image))),
        ("read", "path") => Some(literally(Path(PathPreference::None))),
        ("json", "path") => Some(literally(Path(PathPreference::Json))),
        ("yaml", "path") => Some(literally(Path(PathPreference::Yaml))),
        ("xml", "path") => Some(literally(Path(PathPreference::Xml))),
        ("toml", "path") => Some(literally(Path(PathPreference::Toml))),
        ("raw", "theme") => Some(literally(Path(PathPreference::RawTheme))),
        ("raw", "syntaxes") => Some(literally(Path(PathPreference::RawSyntax))),
        ("bibliography" | "cite", "style") => Some(Ty::iter_union([
            literally(Path(PathPreference::Csl)),
            Ty::from_cast_info(&param.input),
        ])),
        ("cite", "key") => Some(Ty::iter_union([literally(CiteLabel)])),
        ("ref", "target") => Some(Ty::iter_union([literally(RefLabel)])),
        ("link", "dest") | ("footnote", "body") => Some(Ty::iter_union([
            literally(RefLabel),
            Ty::from_cast_info(&param.input),
        ])),
        ("bibliography", "path") => Some(literally(Path(PathPreference::Bibliography))),
        ("text", "size") => Some(literally(TextSize)),
        ("text", "font") => {
            static FONT_TYPE: Lazy<Ty> = Lazy::new(|| {
                Ty::iter_union([literally(TextFont), Ty::Array(literally(TextFont).into())])
            });
            Some(FONT_TYPE.clone())
        }
        ("text", "lang") => Some(literally(TextLang)),
        ("text", "region") => Some(literally(TextRegion)),
        ("text" | "stack", "dir") => Some(literally(Dir)),
        (
            // todo: polygon.regular
            "page" | "highlight" | "text" | "path" | "rect" | "ellipse" | "circle" | "polygon"
            | "box" | "block" | "table" | "regular",
            "fill",
        ) => Some(literally(Color(Option::None))),
        (
            // todo: table.cell
            "table" | "cell" | "block" | "box" | "circle" | "ellipse" | "rect" | "square",
            "inset",
        ) => Some(literally(Inset)),
        ("block" | "box" | "circle" | "ellipse" | "rect" | "square", "outset") => {
            Some(literally(Outset))
        }
        ("block" | "box" | "rect" | "square", "radius") => Some(literally(Radius)),
        ("grid" | "table", "columns" | "rows" | "gutter" | "column-gutter" | "row-gutter") => {
            static COLUMN_TYPE: Lazy<Ty> = Lazy::new(|| {
                flow_union!(
                    Ty::Value(InsTy::new(Value::Auto)),
                    Ty::Value(InsTy::new(Value::Type(Type::of::<i64>()))),
                    literally(Length(Option::None)),
                    Ty::Array(literally(Length(Option::None)).into()),
                )
            });
            Some(COLUMN_TYPE.clone())
        }
        ("pattern", "size") => {
            static PATTERN_SIZE_TYPE: Lazy<Ty> = Lazy::new(|| {
                flow_union!(
                    Ty::Value(InsTy::new(Value::Auto)),
                    Ty::Array(Ty::Lit(Length(Option::None)).into()),
                )
            });
            Some(PATTERN_SIZE_TYPE.clone())
        }
        ("stroke", "dash") => Some(FLOW_STROKE_DASH_TYPE.clone()),
        (
            //todo: table.cell, table.hline, table.vline, math.cancel, grid.cell, polygon.regular
            "cancel" | "highlight" | "overline" | "strike" | "underline" | "text" | "path" | "rect"
            | "ellipse" | "circle" | "polygon" | "box" | "block" | "table" | "line" | "cell"
            | "hline" | "vline" | "regular",
            "stroke",
        ) => Some(Ty::Lit(Stroke)),
        ("page", "margin") => Some(Ty::Lit(Margin)),
        _ => Option::None,
    }
}

static FLOW_STROKE_DASH_TYPE: Lazy<Ty> = Lazy::new(|| {
    flow_union!(
        "solid",
        "dotted",
        "densely-dotted",
        "loosely-dotted",
        "dashed",
        "densely-dashed",
        "loosely-dashed",
        "dash-dotted",
        "densely-dash-dotted",
        "loosely-dash-dotted",
        Ty::Array(flow_union!("dot", literally(Float(Option::None))).into()),
        Ty::Dict(flow_record!(
            "array" => Ty::Array(flow_union!("dot", literally(Float(Option::None))).into()),
            "phase" => literally(Length(Option::None)),
        ))
    )
});

pub static FLOW_STROKE_DICT: Lazy<Interned<RecordTy>> = Lazy::new(|| {
    flow_record!(
        "paint" => literally(Color(Option::None)),
        "thickness" => literally(Length(Option::None)),
        "cap" => flow_union!("butt", "round", "square"),
        "join" => flow_union!("miter", "round", "bevel"),
        "dash" => FLOW_STROKE_DASH_TYPE.clone(),
        "miter-limit" => literally(Float(Option::None)),
    )
});

pub static FLOW_MARGIN_DICT: Lazy<Interned<RecordTy>> = Lazy::new(|| {
    flow_record!(
        "top" => literally(Length(Option::None)),
        "right" => literally(Length(Option::None)),
        "bottom" => literally(Length(Option::None)),
        "left" => literally(Length(Option::None)),
        "inside" => literally(Length(Option::None)),
        "outside" => literally(Length(Option::None)),
        "x" => literally(Length(Option::None)),
        "y" => literally(Length(Option::None)),
        "rest" => literally(Length(Option::None)),
    )
});

pub static FLOW_INSET_DICT: Lazy<Interned<RecordTy>> = Lazy::new(|| {
    flow_record!(
        "top" => literally(Length(Option::None)),
        "right" => literally(Length(Option::None)),
        "bottom" => literally(Length(Option::None)),
        "left" => literally(Length(Option::None)),
        "x" => literally(Length(Option::None)),
        "y" => literally(Length(Option::None)),
        "rest" => literally(Length(Option::None)),
    )
});

pub static FLOW_OUTSET_DICT: Lazy<Interned<RecordTy>> = Lazy::new(|| {
    flow_record!(
        "top" => literally(Length(Option::None)),
        "right" => literally(Length(Option::None)),
        "bottom" => literally(Length(Option::None)),
        "left" => literally(Length(Option::None)),
        "x" => literally(Length(Option::None)),
        "y" => literally(Length(Option::None)),
        "rest" => literally(Length(Option::None)),
    )
});

pub static FLOW_RADIUS_DICT: Lazy<Interned<RecordTy>> = Lazy::new(|| {
    flow_record!(
        "top" => literally(Length(Option::None)),
        "right" => literally(Length(Option::None)),
        "bottom" => literally(Length(Option::None)),
        "left" => literally(Length(Option::None)),
        "top-left" => literally(Length(Option::None)),
        "top-right" => literally(Length(Option::None)),
        "bottom-left" => literally(Length(Option::None)),
        "bottom-right" => literally(Length(Option::None)),
        "rest" => literally(Length(Option::None)),
    )
});

// todo bad case: array.fold
// todo bad case: datetime
// todo bad case: selector
// todo: function signatures, for example: `locate(loc => ...)`

// todo: numbering/supplement
// todo: grid/table.fill/align/stroke/inset can be a function
// todo: math.cancel.angle can be a function
// todo: text.features array/dictionary
// todo: math.mat.augment
// todo: csv.row-type can be an array or a dictionary

// ISO 639

#[cfg(test)]
mod tests {

    use crate::syntax::Decl;

    use super::{SigTy, Ty, TypeVar};

    // todo: map function
    // Technical Note for implementing a map function:
    // `u`, `v` is in level 2
    // instantiate a `v` as the return type of the map function.
    #[test]
    fn test_map() {
        let u = Ty::Var(TypeVar::new("u".into(), Decl::lit("u").into()));
        let v = Ty::Var(TypeVar::new("v".into(), Decl::lit("v").into()));
        let mapper_fn =
            Ty::Func(SigTy::new([u].into_iter(), None, None, None, Some(v.clone())).into());
        let map_fn =
            Ty::Func(SigTy::new([mapper_fn].into_iter(), None, None, None, Some(v)).into());
        let _ = map_fn;
        // println!("{map_fn:?}");
    }
}
