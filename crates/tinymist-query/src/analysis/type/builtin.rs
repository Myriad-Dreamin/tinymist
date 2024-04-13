use ecow::EcoVec;
use once_cell::sync::Lazy;
use typst::syntax::Span;

use super::{FlowRecord, FlowType};

#[derive(Debug, Clone, Hash)]
pub(crate) enum PathPreference {
    None,
    Source,
    Image,
    Json,
    Yaml,
    Xml,
    Toml,
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
        }
    }
}

#[derive(Debug, Clone, Hash)]
pub(crate) enum FlowBuiltinType {
    Args,
    Stroke,
    MarginLike,
    Color,
    TextSize,
    TextFont,
    DirParam,
    Length,
    Float,
    Path(PathPreference),
}

pub static FLOW_STROKE_DICT: Lazy<FlowRecord> = Lazy::new(|| FlowRecord {
    fields: EcoVec::from_iter([
        (
            "paint".into(),
            FlowType::Builtin(FlowBuiltinType::Color),
            Span::detached(),
        ),
        (
            "thickness".into(),
            FlowType::Builtin(FlowBuiltinType::Length),
            Span::detached(),
        ),
        (
            "cap".into(),
            FlowType::Union(Box::new(Vec::from_iter([
                FlowType::from_string("butt".into()),
                FlowType::from_string("round".into()),
                FlowType::from_string("square".into()),
            ]))),
            Span::detached(),
        ),
        (
            "join".into(),
            FlowType::Union(Box::new(Vec::from_iter([
                FlowType::from_string("miter".into()),
                FlowType::from_string("round".into()),
                FlowType::from_string("bevel".into()),
            ]))),
            Span::detached(),
        ),
        (
            "dash".into(),
            FlowType::Union(Box::new(Vec::from_iter([
                FlowType::from_string("solid".into()),
                FlowType::from_string("dotted".into()),
                FlowType::from_string("densely-dotted".into()),
                FlowType::from_string("loosely-dotted".into()),
                FlowType::from_string("dashed".into()),
                FlowType::from_string("densely-dashed".into()),
                FlowType::from_string("loosely-dashed".into()),
                FlowType::from_string("dash-dotted".into()),
                FlowType::from_string("densely-dash-dotted".into()),
                FlowType::from_string("loosely-dash-dotted".into()),
            ]))),
            Span::detached(),
        ),
        (
            "miter-limit".into(),
            FlowType::Builtin(FlowBuiltinType::Float),
            Span::detached(),
        ),
    ]),
});
