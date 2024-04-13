use ecow::EcoVec;
use once_cell::sync::Lazy;
use typst::{
    foundations::{Func, ParamInfo, Value},
    syntax::Span,
};

use super::{FlowRecord, FlowType};

#[derive(Debug, Clone, Hash)]
pub(crate) enum PathPreference {
    None,
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
    pub(crate) fn match_ext(&self, ext: &std::ffi::OsStr) -> bool {
        let ext = || ext.to_str().map(|e| e.to_lowercase()).unwrap_or_default();

        match self {
            PathPreference::None => true,
            PathPreference::Source => {
                matches!(ext().as_ref(), "typ")
            }
            PathPreference::Image => {
                matches!(
                    ext().as_ref(),
                    "png" | "webp" | "jpg" | "jpeg" | "svg" | "svgz"
                )
            }
            PathPreference::Json => {
                matches!(ext().as_ref(), "json" | "jsonc" | "json5")
            }
            PathPreference::Yaml => matches!(ext().as_ref(), "yaml" | "yml"),
            PathPreference::Xml => matches!(ext().as_ref(), "xml"),
            PathPreference::Toml => matches!(ext().as_ref(), "toml"),
            PathPreference::Csv => matches!(ext().as_ref(), "csv"),
            PathPreference::Bibliography => matches!(ext().as_ref(), "yaml" | "yml" | "bib"),
            PathPreference::RawSyntax => {
                matches!(ext().as_ref(), "tmLanguage" | "sublime-syntax")
            }
            PathPreference::RawTheme => matches!(ext().as_ref(), "tmTheme" | "xml"),
        }
    }
}

pub(in crate::analysis::ty) fn param_mapping(f: &Func, p: &ParamInfo) -> Option<FlowType> {
    match (f.name().unwrap(), p.name) {
        ("cbor", "path") => Some(FlowType::Builtin(FlowBuiltinType::Path(
            PathPreference::None,
        ))),
        ("csv", "path") => Some(FlowType::Builtin(FlowBuiltinType::Path(
            PathPreference::Csv,
        ))),
        ("image", "path") => Some(FlowType::Builtin(FlowBuiltinType::Path(
            PathPreference::Image,
        ))),
        ("read", "path") => Some(FlowType::Builtin(FlowBuiltinType::Path(
            PathPreference::None,
        ))),
        ("json", "path") => Some(FlowType::Builtin(FlowBuiltinType::Path(
            PathPreference::Json,
        ))),
        ("yaml", "path") => Some(FlowType::Builtin(FlowBuiltinType::Path(
            PathPreference::Yaml,
        ))),
        ("xml", "path") => Some(FlowType::Builtin(FlowBuiltinType::Path(
            PathPreference::Xml,
        ))),
        ("toml", "path") => Some(FlowType::Builtin(FlowBuiltinType::Path(
            PathPreference::Toml,
        ))),
        ("raw", "theme") => Some(FlowType::Builtin(FlowBuiltinType::Path(
            PathPreference::RawTheme,
        ))),
        ("raw", "syntaxes") => Some(FlowType::Builtin(FlowBuiltinType::Path(
            PathPreference::RawSyntax,
        ))),
        ("bibliography", "path") => Some(FlowType::Builtin(FlowBuiltinType::Path(
            PathPreference::Bibliography,
        ))),
        ("text", "size") => Some(FlowType::Builtin(FlowBuiltinType::TextSize)),
        ("text" | "stack", "dir") => Some(FlowType::Builtin(FlowBuiltinType::Dir)),
        ("text", "font") => Some(FlowType::Builtin(FlowBuiltinType::TextFont)),
        (
            // todo: polygon.regular
            "highlight" | "text" | "path" | "rect" | "ellipse" | "circle" | "polygon" | "box"
            | "block" | "table" | "regular",
            "fill",
        ) => Some(FlowType::Builtin(FlowBuiltinType::Color)),
        (
            // todo: table.cell
            "table" | "cell" | "block" | "box" | "circle" | "ellipse" | "rect" | "square",
            "inset",
        ) => Some(FlowType::Builtin(FlowBuiltinType::Inset)),
        ("block" | "box" | "circle" | "ellipse" | "rect" | "square", "outset") => {
            Some(FlowType::Builtin(FlowBuiltinType::Outset))
        }
        ("block" | "box" | "rect" | "square", "radius") => {
            Some(FlowType::Builtin(FlowBuiltinType::Radius))
        }
        (
            //todo: table.cell, table.hline, table.vline, math.cancel, grid.cell, polygon.regular
            "cancel" | "highlight" | "overline" | "strike" | "underline" | "text" | "path" | "rect"
            | "ellipse" | "circle" | "polygon" | "box" | "block" | "table" | "line" | "cell"
            | "hline" | "vline" | "regular",
            "stroke",
        ) => Some(FlowType::Builtin(FlowBuiltinType::Stroke)),
        ("page", "margin") => Some(FlowType::Builtin(FlowBuiltinType::Margin)),
        _ => None,
    }
}

#[derive(Debug, Clone, Hash)]
pub(crate) enum FlowBuiltinType {
    Args,
    Color,
    TextSize,
    TextFont,

    Dir,
    Length,
    Float,

    Stroke,
    Margin,
    Inset,
    Outset,
    Radius,

    Path(PathPreference),
}

fn literally(s: impl FlowBuiltinLiterally) -> FlowType {
    s.literally()
}

trait FlowBuiltinLiterally {
    fn literally(&self) -> FlowType;
}

impl FlowBuiltinLiterally for &str {
    fn literally(&self) -> FlowType {
        FlowType::Value(Box::new((Value::Str((*self).into()), Span::detached())))
    }
}

impl FlowBuiltinLiterally for FlowBuiltinType {
    fn literally(&self) -> FlowType {
        FlowType::Builtin(self.clone())
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

use FlowBuiltinType::*;

pub static FLOW_STROKE_DICT: Lazy<FlowRecord> = Lazy::new(|| {
    flow_record!(
        "paint" => literally(Color),
        "thickness" => literally(Length),
        "cap" => flow_union!("butt", "round", "square"),
        "join" => flow_union!("miter", "round", "bevel"),
        "dash" => flow_union!(
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
        ),
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
// todo: text.lang
// todo: text.region
// todo: text.font array
// todo: stroke.dash can be an array
// todo: csv.row-type can be an array or a dictionary
