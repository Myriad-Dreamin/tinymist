mod fonts;
mod symbols;

mod prelude {

    pub use std::collections::HashMap;

    pub use once_cell::sync::Lazy;
    pub use reflexo_typst::error::prelude::*;
    pub use reflexo_typst::Compiler;
    pub use reflexo_vec2svg::ir::{GlyphItem, GlyphRef};
    pub use reflexo_vec2svg::{DefaultExportFeature, SvgTask, SvgText};
    pub use serde::{Deserialize, Serialize};
    pub use serde_json::Value as JsonValue;
    pub use sync_lsp::*;
    pub use typst::foundations::{Scope, Value};
    pub use typst::symbols::Symbol;

    pub use crate::LanguageState;

    pub type Svg<'a> = SvgTask<'a, DefaultExportFeature>;
}
