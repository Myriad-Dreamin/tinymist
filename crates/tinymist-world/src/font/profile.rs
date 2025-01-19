use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{collections::HashMap, time::SystemTime};
use typst::text::{Coverage, FontInfo};

type FontMetaDict = HashMap<String, String>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontInfoItem {
    /// customized profile data
    pub meta: FontMetaDict,
    /// The informatioin of the font
    pub info: FontInfo,
}

impl FontInfoItem {
    pub fn new(info: FontInfo) -> Self {
        Self {
            meta: Default::default(),
            info,
        }
    }

    pub fn index(&self) -> Option<u32> {
        self.meta.get("index").and_then(|v| v.parse::<u32>().ok())
    }

    pub fn set_index(&mut self, v: u32) {
        self.meta.insert("index".to_owned(), v.to_string());
    }

    pub fn coverage_hash(&self) -> Option<&String> {
        self.meta.get("coverage_hash")
    }

    pub fn set_coverage_hash(&mut self, v: String) {
        self.meta.insert("coverage_hash".to_owned(), v);
    }

    pub fn meta(&self) -> &FontMetaDict {
        &self.meta
    }

    pub fn info(&self) -> &FontInfo {
        &self.info
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontProfileItem {
    /// The hash of the file
    pub hash: String,
    /// customized profile data
    pub meta: FontMetaDict,
    /// The informatioin of the font
    pub info: Vec<FontInfoItem>,
}

fn to_micro_lossy(t: SystemTime) -> u128 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_micros()
}

impl FontProfileItem {
    pub fn new(kind: &str, hash: String) -> Self {
        let mut meta: FontMetaDict = Default::default();
        meta.insert("kind".to_owned(), kind.to_string());

        Self {
            hash,
            meta,
            info: Default::default(),
        }
    }

    pub fn path(&self) -> Option<&String> {
        self.meta.get("path")
    }

    pub fn mtime(&self) -> Option<SystemTime> {
        self.meta.get("mtime").and_then(|v| {
            let v = v.parse::<u64>().ok();
            v.map(|v| SystemTime::UNIX_EPOCH + std::time::Duration::from_micros(v))
        })
    }

    pub fn mtime_is_exact(&self, t: SystemTime) -> bool {
        self.mtime()
            .map(|s| {
                let s = to_micro_lossy(s);
                let t = to_micro_lossy(t);
                s == t
            })
            .unwrap_or_default()
    }

    pub fn set_path(&mut self, v: String) {
        self.meta.insert("path".to_owned(), v);
    }

    pub fn set_mtime(&mut self, v: SystemTime) {
        self.meta
            .insert("mtime".to_owned(), to_micro_lossy(v).to_string());
    }

    pub fn hash(&self) -> &str {
        &self.hash
    }

    pub fn meta(&self) -> &FontMetaDict {
        &self.meta
    }

    pub fn info(&self) -> &[FontInfoItem] {
        &self.info
    }

    pub fn add_info(&mut self, info: FontInfoItem) {
        self.info.push(info);
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct FontProfile {
    pub version: String,
    pub build_info: String,
    pub items: Vec<FontProfileItem>,
}

pub fn get_font_coverage_hash(coverage: &Coverage) -> String {
    let mut coverage_hash = sha2::Sha256::new();
    coverage
        .iter()
        .for_each(|c| coverage_hash.update(c.to_le_bytes()));
    let coverage_hash = coverage_hash.finalize();
    format!("sha256:{coverage_hash:x}")
}
