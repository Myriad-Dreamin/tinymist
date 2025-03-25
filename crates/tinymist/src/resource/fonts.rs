use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tinymist_std::debug_loc::DataSource;
use typst::text::{FontStretch, FontStyle, FontWeight};

use super::prelude::*;
use crate::project::LspComputeGraph;
use crate::world::font::FontResolver;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FontResourceItem {
    name: String,
    infos: Vec<TypstFontInfo>,
}

/// Information about a font.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypstFontInfo {
    /// The display name of the font, which is computed by this crate and
    /// unnecessary from any fields of the font file.
    pub name: String,
    /// The style of the font.
    pub style: FontStyle,
    /// The weight of the font.
    pub weight: FontWeight,
    /// The stretch of the font.
    pub stretch: FontStretch,
    /// The Fixed Family used by Typst.
    pub fixed_family: Option<String>,
    /// The source of the font.
    pub source: Option<u32>,
    /// The index of the font in the source.
    pub index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FontResourceResult {
    sources: Vec<DataSource>,
    families: Vec<FontResourceItem>,
}

impl ServerState {
    /// Get the all valid fonts
    pub async fn get_font_resources(snap: LspComputeGraph) -> LspResult<JsonValue> {
        // fonts
        let resolver = &snap.world().font_resolver;
        let font_book = resolver.font_book();
        let mut source_map: HashMap<Arc<DataSource>, u32> = HashMap::new();
        let mut sources: Vec<DataSource> = Vec::new();

        let mut internal_source = |source: Arc<DataSource>| -> u32 {
            if let Some(&id) = source_map.get(source.as_ref()) {
                return id;
            }
            let id = sources.len() as u32;
            sources.push(source.as_ref().clone());
            source_map.insert(source, id);
            id
        };

        let families: Vec<FontResourceItem> = font_book
            .families()
            .map(|(name, _infos)| {
                let infos = font_book
                    .select_family(&name.to_lowercase())
                    .flat_map(|id| {
                        let source = resolver.describe_font_by_id(id).map(&mut internal_source);
                        let info = font_book.info(id)?;

                        Some(TypstFontInfo {
                            name: info.family.clone(),
                            style: info.variant.style,
                            weight: info.variant.weight,
                            stretch: info.variant.stretch,
                            fixed_family: Some(info.family.clone()),
                            source,
                            index: Some(id as u32),
                        })
                    });
                FontResourceItem {
                    name: name.into(),
                    infos: infos.collect(),
                }
            })
            .collect();

        let result = FontResourceResult { sources, families };
        serde_json::to_value(result).map_err(internal_error)
    }
}
