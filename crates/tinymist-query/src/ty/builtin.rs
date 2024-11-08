use core::fmt;

use once_cell::sync::Lazy;
use regex::RegexSet;
use strum::{EnumIter, IntoEnumIterator};
use typst::foundations::CastInfo;
use typst::{
    foundations::{AutoValue, Content, Func, NoneValue, ParamInfo, Type, Value},
    layout::Length,
};

use crate::ty::*;

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, EnumIter)]
pub enum PathPreference {
    Source,
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
            PathPreference::Source => &SOURCE_REGSET,
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
        PathPreference::iter().find(|p| p.ext_matcher().is_match(path))
    }
}

impl Ty {
    pub(crate) fn from_cast_info(s: &CastInfo) -> Ty {
        match &s {
            CastInfo::Any => Ty::Any,
            CastInfo::Value(v, doc) => Ty::Value(InsTy::new_doc(v.clone(), *doc)),
            CastInfo::Type(ty) => Ty::Builtin(BuiltinTy::Type(*ty)),
            CastInfo::Union(e) => {
                Ty::iter_union(UnionIter(vec![e.as_slice().iter()]).map(Self::from_cast_info))
            }
        }
    }

    pub(crate) fn from_param_site(f: &Func, p: &ParamInfo) -> Ty {
        use typst::foundations::func::Repr;
        match f.inner() {
            Repr::Element(..) | Repr::Native(..) => {
                if let Some(ty) = param_mapping(f, p) {
                    return ty;
                }
            }
            Repr::Closure(_) => {}
            Repr::With(w) => return Ty::from_param_site(&w.0, p),
        };

        Self::from_cast_info(&p.input)
    }

    pub(crate) fn from_return_site(f: &Func, c: &'_ CastInfo) -> Self {
        use typst::foundations::func::Repr;
        match f.inner() {
            Repr::Element(e) => return Ty::Builtin(BuiltinTy::Element(*e)),
            Repr::Closure(_) => {}
            Repr::With(w) => return Ty::from_return_site(&w.0, c),
            Repr::Native(_) => {}
        };

        Self::from_cast_info(c)
    }
}

struct UnionIter<'a>(Vec<std::slice::Iter<'a, CastInfo>>);

impl<'a> Iterator for UnionIter<'a> {
    type Item = &'a CastInfo;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let iter = self.0.last_mut()?;
            if let Some(e) = iter.next() {
                match e {
                    CastInfo::Union(e) => {
                        self.0.push(e.as_slice().iter());
                    }
                    _ => return Some(e),
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

#[derive(Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum BuiltinTy {
    Clause,
    Undef,
    Content,
    Space,
    None,
    Break,
    Continue,
    Infer,
    FlowNone,
    Auto,

    Args,
    Color,
    TextSize,
    TextFont,
    TextLang,
    TextRegion,

    Label,
    CiteLabel,
    RefLabel,
    Dir,
    Length,
    Float,

    Stroke,
    Margin,
    Inset,
    Outset,
    Radius,

    Tag(Box<(StrRef, Option<Interned<PackageId>>)>),
    Type(typst::foundations::Type),
    Element(typst::foundations::Element),
    Path(PathPreference),
}

impl fmt::Debug for BuiltinTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuiltinTy::Clause => f.write_str("Clause"),
            BuiltinTy::Undef => f.write_str("Undef"),
            BuiltinTy::Content => f.write_str("Content"),
            BuiltinTy::Space => f.write_str("Space"),
            BuiltinTy::None => f.write_str("None"),
            BuiltinTy::Break => f.write_str("Break"),
            BuiltinTy::Continue => f.write_str("Continue"),
            BuiltinTy::Infer => f.write_str("Infer"),
            BuiltinTy::FlowNone => f.write_str("FlowNone"),
            BuiltinTy::Auto => f.write_str("Auto"),

            BuiltinTy::Args => write!(f, "Args"),
            BuiltinTy::Color => write!(f, "Color"),
            BuiltinTy::TextSize => write!(f, "TextSize"),
            BuiltinTy::TextFont => write!(f, "TextFont"),
            BuiltinTy::TextLang => write!(f, "TextLang"),
            BuiltinTy::TextRegion => write!(f, "TextRegion"),
            BuiltinTy::Dir => write!(f, "Dir"),
            BuiltinTy::Length => write!(f, "Length"),
            BuiltinTy::Label => write!(f, "Label"),
            BuiltinTy::CiteLabel => write!(f, "CiteLabel"),
            BuiltinTy::RefLabel => write!(f, "RefLabel"),
            BuiltinTy::Float => write!(f, "Float"),
            BuiltinTy::Stroke => write!(f, "Stroke"),
            BuiltinTy::Margin => write!(f, "Margin"),
            BuiltinTy::Inset => write!(f, "Inset"),
            BuiltinTy::Outset => write!(f, "Outset"),
            BuiltinTy::Radius => write!(f, "Radius"),
            BuiltinTy::Type(ty) => write!(f, "Type({})", ty.short_name()),
            BuiltinTy::Element(e) => e.fmt(f),
            BuiltinTy::Tag(tag) => {
                let (name, id) = tag.as_ref();
                if let Some(id) = id {
                    write!(f, "Tag({name:?}) of {id:?}")
                } else {
                    write!(f, "Tag({name:?})")
                }
            }
            BuiltinTy::Path(p) => write!(f, "Path({p:?})"),
        }
    }
}

impl BuiltinTy {
    pub fn from_value(builtin: &Value) -> Ty {
        if let Value::Bool(v) = builtin {
            return Ty::Boolean(Some(*v));
        }

        Self::from_builtin(builtin.ty())
    }

    pub fn from_builtin(builtin: Type) -> Ty {
        if builtin == Type::of::<AutoValue>() {
            return Ty::Builtin(BuiltinTy::Auto);
        }
        if builtin == Type::of::<NoneValue>() {
            return Ty::Builtin(BuiltinTy::None);
        }
        if builtin == Type::of::<typst::visualize::Color>() {
            return Color.literally();
        }
        if builtin == Type::of::<bool>() {
            return Ty::Builtin(BuiltinTy::None);
        }
        if builtin == Type::of::<f64>() {
            return Float.literally();
        }
        if builtin == Type::of::<Length>() {
            return Length.literally();
        }
        if builtin == Type::of::<Content>() {
            return Ty::Builtin(BuiltinTy::Content);
        }

        BuiltinTy::Type(builtin).literally()
    }

    pub(crate) fn describe(&self) -> String {
        let res = match self {
            BuiltinTy::Clause => "any",
            BuiltinTy::Undef => "any",
            BuiltinTy::Content => "content",
            BuiltinTy::Space => "content",
            BuiltinTy::None => "none",
            BuiltinTy::Break => "break",
            BuiltinTy::Continue => "continue",
            BuiltinTy::Infer => "any",
            BuiltinTy::FlowNone => "none",
            BuiltinTy::Auto => "auto",

            BuiltinTy::Args => "arguments",
            BuiltinTy::Color => "color",
            BuiltinTy::TextSize => "text.size",
            BuiltinTy::TextFont => "text.font",
            BuiltinTy::TextLang => "text.lang",
            BuiltinTy::TextRegion => "text.region",
            BuiltinTy::Dir => "dir",
            BuiltinTy::Length => "length",
            BuiltinTy::Float => "float",
            BuiltinTy::Label => "label",
            BuiltinTy::CiteLabel => "cite-label",
            BuiltinTy::RefLabel => "ref-label",
            BuiltinTy::Stroke => "stroke",
            BuiltinTy::Margin => "margin",
            BuiltinTy::Inset => "inset",
            BuiltinTy::Outset => "outset",
            BuiltinTy::Radius => "radius",
            BuiltinTy::Type(ty) => ty.short_name(),
            BuiltinTy::Element(ty) => ty.name(),
            BuiltinTy::Tag(tag) => {
                let (name, id) = tag.as_ref();
                return if let Some(id) = id {
                    format!("tag {name} of {id:?}")
                } else {
                    format!("tag {name}")
                };
            }
            BuiltinTy::Path(s) => match s {
                PathPreference::None => "[any]",
                PathPreference::Special => "[any]",
                PathPreference::Source => "[source]",
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
        };

        res.to_string()
    }
}

use BuiltinTy::*;

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

impl FlowBuiltinLiterally for BuiltinTy {
    fn literally(self) -> Ty {
        Ty::Builtin(self.clone())
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

pub(super) fn param_mapping(f: &Func, p: &ParamInfo) -> Option<Ty> {
    match (f.name()?, p.name) {
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
            Ty::from_cast_info(&p.input),
        ])),
        ("cite", "key") => Some(Ty::iter_union([literally(CiteLabel)])),
        ("ref", "target") => Some(Ty::iter_union([literally(RefLabel)])),
        ("link", "dest") | ("footnote", "body") => Some(Ty::iter_union([
            literally(RefLabel),
            Ty::from_cast_info(&p.input),
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
        ) => Some(literally(Color)),
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
                    literally(Length),
                    Ty::Array(literally(Length).into()),
                )
            });
            Some(COLUMN_TYPE.clone())
        }
        ("pattern", "size") => {
            static PATTERN_SIZE_TYPE: Lazy<Ty> = Lazy::new(|| {
                flow_union!(
                    Ty::Value(InsTy::new(Value::Auto)),
                    Ty::Array(Ty::Builtin(Length).into()),
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
        ) => Some(Ty::Builtin(Stroke)),
        ("page", "margin") => Some(Ty::Builtin(Margin)),
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
        Ty::Array(flow_union!("dot", literally(Float)).into()),
        Ty::Dict(flow_record!(
            "array" => Ty::Array(flow_union!("dot", literally(Float)).into()),
            "phase" => literally(Length),
        ))
    )
});

pub static FLOW_STROKE_DICT: Lazy<Interned<RecordTy>> = Lazy::new(|| {
    flow_record!(
        "paint" => literally(Color),
        "thickness" => literally(Length),
        "cap" => flow_union!("butt", "round", "square"),
        "join" => flow_union!("miter", "round", "bevel"),
        "dash" => FLOW_STROKE_DASH_TYPE.clone(),
        "miter-limit" => literally(Float),
    )
});

pub static FLOW_MARGIN_DICT: Lazy<Interned<RecordTy>> = Lazy::new(|| {
    flow_record!(
        "top" => literally(Length),
        "right" => literally(Length),
        "bottom" => literally(Length),
        "left" => literally(Length),
        "inside" => literally(Length),
        "outside" => literally(Length),
        "x" => literally(Length),
        "y" => literally(Length),
        "rest" => literally(Length),
    )
});

pub static FLOW_INSET_DICT: Lazy<Interned<RecordTy>> = Lazy::new(|| {
    flow_record!(
        "top" => literally(Length),
        "right" => literally(Length),
        "bottom" => literally(Length),
        "left" => literally(Length),
        "x" => literally(Length),
        "y" => literally(Length),
        "rest" => literally(Length),
    )
});

pub static FLOW_OUTSET_DICT: Lazy<Interned<RecordTy>> = Lazy::new(|| {
    flow_record!(
        "top" => literally(Length),
        "right" => literally(Length),
        "bottom" => literally(Length),
        "left" => literally(Length),
        "x" => literally(Length),
        "y" => literally(Length),
        "rest" => literally(Length),
    )
});

pub static FLOW_RADIUS_DICT: Lazy<Interned<RecordTy>> = Lazy::new(|| {
    flow_record!(
        "top" => literally(Length),
        "right" => literally(Length),
        "bottom" => literally(Length),
        "left" => literally(Length),
        "top-left" => literally(Length),
        "top-right" => literally(Length),
        "bottom-left" => literally(Length),
        "bottom-right" => literally(Length),
        "rest" => literally(Length),
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
