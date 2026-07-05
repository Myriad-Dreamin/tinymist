//! The profile of the font.

use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{collections::HashMap, time::SystemTime};
use typst::text::{Coverage, FontInfo};

/// The metadata of the font.
type FontMetaDict = HashMap<String, String>;

/// The item of the font profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontInfoItem {
    /// The metadata of the font.
    pub meta: FontMetaDict,
    /// The information of the font.
    pub info: FontInfo,
}

impl FontInfoItem {
    /// Creates a new font info item.
    pub fn new(info: FontInfo) -> Self {
        Self {
            meta: Default::default(),
            info,
        }
    }

    /// Gets the index of the font.
    pub fn index(&self) -> Option<u32> {
        self.meta.get("index").and_then(|v| v.parse::<u32>().ok())
    }

    /// Sets the index of the font.
    pub fn set_index(&mut self, v: u32) {
        self.meta.insert("index".to_owned(), v.to_string());
    }

    /// Gets the coverage hash of the font.
    pub fn coverage_hash(&self) -> Option<&String> {
        self.meta.get("coverage_hash")
    }

    /// Sets the coverage hash of the font.
    pub fn set_coverage_hash(&mut self, v: String) {
        self.meta.insert("coverage_hash".to_owned(), v);
    }

    /// Gets the metadata of the font.
    pub fn meta(&self) -> &FontMetaDict {
        &self.meta
    }

    /// Gets the information of the font.
    pub fn info(&self) -> &FontInfo {
        &self.info
    }
}

/// The item of the font profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontProfileItem {
    /// The hash of the file.
    pub hash: String,
    /// The metadata of the font.
    pub meta: FontMetaDict,
    /// The information of the font.
    pub info: Vec<FontInfoItem>,
}

/// Converts a system time to a microsecond lossy value.
fn to_micro_lossy(t: SystemTime) -> u128 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_micros()
}

impl FontProfileItem {
    /// Creates a new font profile item.
    pub fn new(kind: &str, hash: String) -> Self {
        let mut meta: FontMetaDict = Default::default();
        meta.insert("kind".to_owned(), kind.to_string());

        Self {
            hash,
            meta,
            info: Default::default(),
        }
    }

    /// Gets the path of the font.
    pub fn path(&self) -> Option<&String> {
        self.meta.get("path")
    }

    /// Gets the modification time of the font.
    pub fn mtime(&self) -> Option<SystemTime> {
        self.meta.get("mtime").and_then(|v| {
            let v = v.parse::<u64>().ok();
            v.map(|v| SystemTime::UNIX_EPOCH + tinymist_std::time::Duration::from_micros(v))
        })
    }

    /// Checks if the modification time is exact.
    pub fn mtime_is_exact(&self, t: SystemTime) -> bool {
        self.mtime()
            .map(|s| {
                let s = to_micro_lossy(s);
                let t = to_micro_lossy(t);
                s == t
            })
            .unwrap_or_default()
    }

    /// Sets the path of the font.
    pub fn set_path(&mut self, v: String) {
        self.meta.insert("path".to_owned(), v);
    }

    /// Sets the modification time of the font.
    pub fn set_mtime(&mut self, v: SystemTime) {
        self.meta
            .insert("mtime".to_owned(), to_micro_lossy(v).to_string());
    }

    /// Gets the hash of the font.
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// Gets the metadata of the font.
    pub fn meta(&self) -> &FontMetaDict {
        &self.meta
    }

    /// Gets the information of the font.
    pub fn info(&self) -> &[FontInfoItem] {
        &self.info
    }

    /// Adds an information of the font.
    pub fn add_info(&mut self, info: FontInfoItem) {
        self.info.push(info);
    }
}

/// The profile of the font.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct FontProfile {
    /// The version of the profile.
    pub version: String,
    /// The build information of the profile.
    pub build_info: String,
    /// The items of the profile.
    pub items: Vec<FontProfileItem>,
}

/// Gets the coverage hash of the font.
pub fn get_font_coverage_hash(coverage: &Coverage) -> String {
    let mut coverage_hash = sha2::Sha256::new();
    coverage
        .iter()
        .for_each(|c| coverage_hash.update(c.to_le_bytes()));
    let coverage_hash = coverage_hash.finalize();
    format!("sha256:{coverage_hash:x}")
}
