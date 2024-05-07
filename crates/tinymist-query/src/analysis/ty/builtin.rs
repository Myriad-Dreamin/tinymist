use ecow::EcoVec;
use once_cell::sync::Lazy;
use regex::RegexSet;
use typst::{
    foundations::{AutoValue, Content, Func, NoneValue, ParamInfo, Type, Value},
    layout::Length,
    syntax::Span,
};

use super::{FlowRecord, FlowType};

#[derive(Debug, Clone, Hash)]
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

#[derive(Debug, Clone, Hash)]
pub(crate) enum FlowBuiltinType {
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
    Path(PathPreference),
}

impl FlowBuiltinType {
    pub fn from_value(builtin: &Value) -> FlowType {
        if let Value::Bool(v) = builtin {
            return FlowType::Boolean(Some(*v));
        }

        Self::from_builtin(builtin.ty())
    }

    pub fn from_builtin(builtin: Type) -> FlowType {
        if builtin == Type::of::<AutoValue>() {
            return FlowType::Auto;
        }
        if builtin == Type::of::<NoneValue>() {
            return FlowType::None;
        }
        if builtin == Type::of::<typst::visualize::Color>() {
            return Color.literally();
        }
        if builtin == Type::of::<bool>() {
            return FlowType::None;
        }
        if builtin == Type::of::<f64>() {
            return Float.literally();
        }
        if builtin == Type::of::<Length>() {
            return Length.literally();
        }
        if builtin == Type::of::<Content>() {
            return FlowType::Content;
        }

        FlowBuiltinType::Type(builtin).literally()
    }

    pub(crate) fn describe(&self) -> &'static str {
        match self {
            FlowBuiltinType::Args => "args",
            FlowBuiltinType::Color => "color",
            FlowBuiltinType::TextSize => "text.size",
            FlowBuiltinType::TextFont => "text.font",
            FlowBuiltinType::TextLang => "text.lang",
            FlowBuiltinType::TextRegion => "text.region",
            FlowBuiltinType::Dir => "dir",
            FlowBuiltinType::Length => "length",
            FlowBuiltinType::Float => "float",
            FlowBuiltinType::Stroke => "stroke",
            FlowBuiltinType::Margin => "margin",
            FlowBuiltinType::Inset => "inset",
            FlowBuiltinType::Outset => "outset",
            FlowBuiltinType::Radius => "radius",
            FlowBuiltinType::Type(ty) => ty.short_name(),
            FlowBuiltinType::Path(s) => match s {
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

use FlowBuiltinType::*;

fn literally(s: impl FlowBuiltinLiterally) -> FlowType {
    s.literally()
}

trait FlowBuiltinLiterally {
    fn literally(self) -> FlowType;
}

impl FlowBuiltinLiterally for &str {
    fn literally(self) -> FlowType {
        FlowType::Value(Box::new((Value::Str((*self).into()), Span::detached())))
    }
}

impl FlowBuiltinLiterally for FlowBuiltinType {
    fn literally(self) -> FlowType {
        FlowType::Builtin(self.clone())
    }
}

impl FlowBuiltinLiterally for FlowType {
    fn literally(self) -> FlowType {
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
        FlowType::Union(Box::new(flow_builtin_union_inner!( $($b)* )))
    };

}

macro_rules! flow_record {
    ($($name:expr => $ty:expr),* $(,)?) => {
        FlowRecord {
            fields: EcoVec::from_iter([
                $(
                    (
                        $name.into(),
                        $ty,
                        Span::detached(),
                    ),
                )*
            ])
        }
    };
}

pub(in crate::analysis::ty) fn param_mapping(f: &Func, p: &ParamInfo) -> Option<FlowType> {
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
            static FONT_TYPE: Lazy<FlowType> = Lazy::new(|| {
                FlowType::Union(Box::new(vec![
                    literally(TextFont),
                    FlowType::Array(Box::new(literally(TextFont))),
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
            static COLUMN_TYPE: Lazy<FlowType> = Lazy::new(|| {
                flow_union!(
                    FlowType::Value(Box::new((Value::Auto, Span::detached()))),
                    FlowType::Value(Box::new((Value::Type(Type::of::<i64>()), Span::detached()))),
                    literally(Length),
                    FlowType::Array(Box::new(literally(Length))),
                )
            });
            Some(COLUMN_TYPE.clone())
        }
        ("pattern", "size") => {
            static PATTERN_SIZE_TYPE: Lazy<FlowType> = Lazy::new(|| {
                flow_union!(
                    FlowType::Value(Box::new((Value::Auto, Span::detached()))),
                    FlowType::Array(Box::new(FlowType::Builtin(Length))),
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
        ) => Some(FlowType::Builtin(Stroke)),
        ("page", "margin") => Some(FlowType::Builtin(Margin)),
        _ => None,
    }
}

static FLOW_STROKE_DASH_TYPE: Lazy<FlowType> = Lazy::new(|| {
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
        FlowType::Array(Box::new(flow_union!("dot", literally(Float)))),
        FlowType::Dict(flow_record!(
            "array" => FlowType::Array(Box::new(flow_union!("dot", literally(Float)))),
            "phase" => literally(Length),
        ))
    )
});

pub static FLOW_STROKE_DICT: Lazy<FlowRecord> = Lazy::new(|| {
    flow_record!(
        "paint" => literally(Color),
        "thickness" => literally(Length),
        "cap" => flow_union!("butt", "round", "square"),
        "join" => flow_union!("miter", "round", "bevel"),
        "dash" => FLOW_STROKE_DASH_TYPE.clone(),
        "miter-limit" => literally(Float),
    )
});

pub static FLOW_MARGIN_DICT: Lazy<FlowRecord> = Lazy::new(|| {
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

pub static FLOW_INSET_DICT: Lazy<FlowRecord> = Lazy::new(|| {
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

pub static FLOW_OUTSET_DICT: Lazy<FlowRecord> = Lazy::new(|| {
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

pub static FLOW_RADIUS_DICT: Lazy<FlowRecord> = Lazy::new(|| {
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
