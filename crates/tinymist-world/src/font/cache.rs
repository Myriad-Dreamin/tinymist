use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use typst::text::FontInfo;

#[derive(Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
pub enum CacheCondition {
    Sha256(String),
}

#[derive(Serialize, Deserialize)]
pub struct FontInfoCache {
    pub info: Vec<FontInfo>,
    pub conditions: Vec<CacheCondition>,
}

impl FontInfoCache {
    pub fn from_data(buffer: &[u8]) -> Self {
        let hash = hex::encode(Sha256::digest(buffer));

        FontInfoCache {
            info: FontInfo::iter(buffer).collect(),
            conditions: vec![CacheCondition::Sha256(hash)],
        }
    }
}
