//! Analyzing the syntax of a source file.
//!
//! This module must hide all **AST details** from the rest of the codebase.

// todo: remove this
#![allow(missing_docs)]

use ecow::EcoString;
pub use tinymist_analysis::import::*;
pub(crate) mod lexical_hierarchy;
pub use lexical_hierarchy::*;
pub(crate) mod matcher;
pub use matcher::*;
pub(crate) mod module;
pub use module::*;
pub(crate) mod comment;
pub use comment::*;
pub(crate) mod expr;
pub use expr::*;
pub(crate) mod docs;
pub use docs::*;

use core::fmt;
use std::ops::Range;

use serde::{Deserialize, Serialize};

/// A flat and transient reference to some symbol in a source file.
///
/// It is transient because it is not guaranteed to be valid after the source
/// file is modified.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct IdentRef {
    /// The name of the symbol.
    pub name: EcoString,
    /// The byte range of the symbol in the source file.
    pub range: Range<usize>,
}

impl PartialOrd for IdentRef {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IdentRef {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name
            .cmp(&other.name)
            .then_with(|| self.range.start.cmp(&other.range.start))
    }
}

impl fmt::Display for IdentRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{:?}", self.name, self.range)
    }
}

impl Serialize for IdentRef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let s = self.to_string();
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for IdentRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let (name, range) = {
            let mut parts = s.split('@');
            let name = parts.next().ok_or_else(|| {
                serde::de::Error::custom("expected name@range, but found empty string")
            })?;
            let range = parts.next().ok_or_else(|| {
                serde::de::Error::custom("expected name@range, but found no range")
            })?;
            // let range = range
            //     .parse()
            //     .map_err(|e| serde::de::Error::custom(format!("failed to parse range:
            // {}", e)))?;
            let st_ed = range
                .split("..")
                .map(|s| {
                    s.parse().map_err(|e| {
                        serde::de::Error::custom(format!("failed to parse range: {e}"))
                    })
                })
                .collect::<Result<Vec<usize>, _>>()?;
            if st_ed.len() != 2 {
                return Err(serde::de::Error::custom("expected range to have 2 parts"));
            }
            (name, st_ed[0]..st_ed[1])
        };
        Ok(IdentRef {
            name: name.into(),
            range,
        })
    }
}

/// A flat and transient reference to some symbol in a source file.
///
/// See [`IdentRef`] for definition of a "transient" reference.
#[derive(Debug, Clone, Serialize)]
pub struct IdentDef {
    /// The name of the symbol.
    pub name: EcoString,
    /// The kind of the symbol.
    pub kind: LexicalKind,
    /// The byte range of the symbol in the source file.
    pub range: Range<usize>,
}
