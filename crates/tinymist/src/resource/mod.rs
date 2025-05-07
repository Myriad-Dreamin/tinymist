mod fonts;
mod symbols;

mod prelude {

    pub use std::collections::HashMap;

    pub use reflexo_vec2svg::ir::{GlyphItem, GlyphRef};
    pub use reflexo_vec2svg::{DefaultExportFeature, SvgTask, SvgText};
    pub use serde::{Deserialize, Serialize};
    pub use serde_json::Value as JsonValue;
    pub use sync_ls::*;
    pub use tinymist_std::error::prelude::*;
    pub use typst::foundations::{Scope, Symbol, Value};

    pub(crate) use crate::ServerState;

    pub type Svg<'a> = SvgTask<'a, DefaultExportFeature>;
}
