mod symbols;

mod prelude {

    pub use std::collections::HashMap;

    pub use once_cell::sync::Lazy;
    pub use serde::{Deserialize, Serialize};
    pub use serde_json::Value as JsonValue;
    pub use typst::foundations::{Scope, Value};
    pub use typst::symbols::Symbol;
    pub use typst_ts_compiler::service::Compiler;
    pub use typst_ts_core::error::prelude::*;
    pub use typst_ts_svg_exporter::ir::{GlyphItem, GlyphRef};
    pub use typst_ts_svg_exporter::{DefaultExportFeature, SvgTask, SvgText};

    pub use crate::TypstLanguageServer;

    pub type Svg<'a> = SvgTask<'a, DefaultExportFeature>;
}
