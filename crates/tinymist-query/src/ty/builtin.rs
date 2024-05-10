use once_cell::sync::Lazy;
use regex::RegexSet;
use typst::{foundations::CastInfo, syntax::Span};
use typst::{
    foundations::{AutoValue, Content, Func, NoneValue, ParamInfo, Type, Value},
    layout::Length,
};

use crate::{adt::interner::Interned, ty::*};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) enum PathPreference {
    None,
    Special,
    Source,
    Csv,
    Image,
    Json,
    Yaml,
    Xml,
    Toml,
    Bibliography,
    RawTheme,
    RawSyntax,
}

impl PathPreference {
    pub fn ext_matcher(&self) -> &'static RegexSet {
        static SOURCE_REGSET: Lazy<RegexSet> =
            Lazy::new(|| RegexSet::new([r"^typ$", r"^typc$"]).unwrap());
        static IMAGE_REGSET: Lazy<RegexSet> = Lazy::new(|| {
            RegexSet::new([
                r"^png$", r"^webp$", r"^jpg$", r"^jpeg$", r"^svg$", r"^svgz$",
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
                let patterns = patterns.chain(RAW_THEME_REGSET.patterns());
                patterns.chain(RAW_SYNTAX_REGSET.patterns())
            })
            .unwrap()
        });

        match self {
            PathPreference::None => &ALL_REGSET,
            PathPreference::Special => &ALL_SPECIAL_REGSET,
            PathPreference::Source => &SOURCE_REGSET,
            PathPreference::Csv => &CSV_REGSET,
            PathPreference::Image => &IMAGE_REGSET,
            PathPreference::Json => &JSON_REGSET,
            PathPreference::Yaml => &YAML_REGSET,
            PathPreference::Xml => &XML_REGSET,
            PathPreference::Toml => &TOML_REGSET,
            PathPreference::Bibliography => &BIB_REGSET,
            PathPreference::RawTheme => &RAW_THEME_REGSET,
            PathPreference::RawSyntax => &RAW_SYNTAX_REGSET,
        }
    }
}

impl Ty {
    pub fn from_return_site(f: &Func, c: &'_ CastInfo) -> Option<Self> {
        use typst::foundations::func::Repr;
        match f.inner() {
            Repr::Element(e) => return Some(Ty::Builtin(BuiltinTy::Element(*e))),
            Repr::Closure(_) => {}
            Repr::With(w) => return Ty::from_return_site(&w.0, c),
            Repr::Native(_) => {}
        };

        let ty = match c {
            CastInfo::Any => Ty::Any,
            CastInfo::Value(v, doc) => Ty::Value(InsTy::new_doc(v.clone(), doc)),
            CastInfo::Type(ty) => Ty::Builtin(BuiltinTy::Type(*ty)),
            CastInfo::Union(e) => {
                // flat union
                let e = UnionIter(vec![e.as_slice().iter()]);

                Ty::Union(Interned::new(
                    e.flat_map(|e| Self::from_return_site(f, e)).collect(),
                ))
            }
        };

        Some(ty)
    }

    pub(crate) fn from_param_site(f: &Func, p: &ParamInfo, s: &CastInfo) -> Option<Ty> {
        use typst::foundations::func::Repr;
        match f.inner() {
            Repr::Element(..) | Repr::Native(..) => {
                if let Some(ty) = param_mapping(f, p) {
                    return Some(ty);
                }
            }
            Repr::Closure(_) => {}
            Repr::With(w) => return Ty::from_param_site(&w.0, p, s),
        };

        let ty = match &s {
            CastInfo::Any => Ty::Any,
            CastInfo::Value(v, doc) => Ty::Value(InsTy::new_doc(v.clone(), doc)),
            CastInfo::Type(ty) => Ty::Builtin(BuiltinTy::Type(*ty)),
            CastInfo::Union(e) => {
                // flat union
                let e = UnionIter(vec![e.as_slice().iter()]);

                Ty::Union(Interned::new(
                    e.flat_map(|e| Self::from_param_site(f, p, e)).collect(),
                ))
            }
        };

        Some(ty)
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

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) enum BuiltinTy {
    Args,
    Color,
    TextSize,
    TextFont,
    TextLang,
    TextRegion,

    Dir,
    Length,
    Float,

    Stroke,
    Margin,
    Inset,
    Outset,
    Radius,

    Type(typst::foundations::Type),
    Element(typst::foundations::Element),
    Path(PathPreference),
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
            return Ty::Auto;
        }
        if builtin == Type::of::<NoneValue>() {
            return Ty::None;
        }
        if builtin == Type::of::<typst::visualize::Color>() {
            return Color.literally();
        }
        if builtin == Type::of::<bool>() {
            return Ty::None;
        }
        if builtin == Type::of::<f64>() {
            return Float.literally();
        }
        if builtin == Type::of::<Length>() {
            return Length.literally();
        }
        if builtin == Type::of::<Content>() {
            return Ty::Content;
        }

        BuiltinTy::Type(builtin).literally()
    }

    pub(crate) fn describe(&self) -> &'static str {
        match self {
            BuiltinTy::Args => "args",
            BuiltinTy::Color => "color",
            BuiltinTy::TextSize => "text.size",
            BuiltinTy::TextFont => "text.font",
            BuiltinTy::TextLang => "text.lang",
            BuiltinTy::TextRegion => "text.region",
            BuiltinTy::Dir => "dir",
            BuiltinTy::Length => "length",
            BuiltinTy::Float => "float",
            BuiltinTy::Stroke => "stroke",
            BuiltinTy::Margin => "margin",
            BuiltinTy::Inset => "inset",
            BuiltinTy::Outset => "outset",
            BuiltinTy::Radius => "radius",
            BuiltinTy::Type(ty) => ty.short_name(),
            BuiltinTy::Element(ty) => ty.name(),
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
                PathPreference::Bibliography => "[bib]",
                PathPreference::RawTheme => "[theme]",
                PathPreference::RawSyntax => "[syntax]",
            },
        }
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
        Ty::Union(Interned::new(flow_builtin_union_inner!( $($b)* )))
    };

}

macro_rules! flow_record {
    ($($name:expr => $ty:expr),* $(,)?) => {
        RecordTy::new(vec![
            $(
                (
                    $name.into(),
                    $ty,
                    Span::detached(),
                ),
            )*
        ])
    };
}

pub(super) fn param_mapping(f: &Func, p: &ParamInfo) -> Option<Ty> {
    match (f.name().unwrap(), p.name) {
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
        ("bibliography", "path") => Some(literally(Path(PathPreference::Bibliography))),
        ("text", "size") => Some(literally(TextSize)),
        ("text", "font") => {
            static FONT_TYPE: Lazy<Ty> = Lazy::new(|| {
                Ty::Union(Interned::new(vec![
                    literally(TextFont),
                    Ty::Array(Interned::new(literally(TextFont))),
                ]))
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
                    Ty::Array(Interned::new(literally(Length))),
                )
            });
            Some(COLUMN_TYPE.clone())
        }
        ("pattern", "size") => {
            static PATTERN_SIZE_TYPE: Lazy<Ty> = Lazy::new(|| {
                flow_union!(
                    Ty::Value(InsTy::new(Value::Auto)),
                    Ty::Array(Interned::new(Ty::Builtin(Length))),
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
        _ => None,
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
        Ty::Array(Interned::new(flow_union!("dot", literally(Float)))),
        Ty::Dict(flow_record!(
            "array" => Ty::Array(Interned::new(flow_union!("dot", literally(Float)))),
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

// todo bad case: function.with
// todo bad case: function.where
// todo bad case: array.fold
// todo bad case: datetime
// todo bad case: selector
// todo: function signatures, for example: `locate(loc => ...)`

// todo: numbering/supplement
// todo: grid/table.columns/rows/gutter/column-gutter/row-gutter array of length
// todo: pattern.size array of length
// todo: grid/table.fill/align/stroke/inset can be a function
// todo: math.cancel.angle can be a function
// todo: text.features array/dictionary
// todo: math.mat.augment
// todo: csv.row-type can be an array or a dictionary

// ISO 639
