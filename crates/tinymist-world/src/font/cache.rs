//! The cache of the font info.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use typst::text::FontInfo;

/// The condition of the cache.
#[derive(Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
pub enum CacheCondition {
    /// The sha256 hash of the data.
    Sha256(String),
}

/// The cache of the font info.
#[derive(Serialize, Deserialize)]
pub struct FontInfoCache {
    /// The font info.
    pub info: Vec<FontInfo>,
    /// The conditions of the cache.
    pub conditions: Vec<CacheCondition>,
}

impl FontInfoCache {
    /// Creates a new font info cache from the data.
    pub fn from_data(buffer: &[u8]) -> Self {
        let hash = hex::encode(Sha256::digest(buffer));

        FontInfoCache {
            info: FontInfo::iter(buffer).collect(),
            conditions: vec![CacheCondition::Sha256(hash)],
        }
    }
}
