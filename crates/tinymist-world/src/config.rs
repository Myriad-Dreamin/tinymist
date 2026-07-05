//! The configuration of the world.

use std::borrow::Cow;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use tinymist_std::AsCowBytes;
use typst::foundations::Dict;

use crate::EntryOpts;

/// The options to create the world.
#[serde_as]
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CompileOpts {
    /// The path to the entry.
    pub entry: EntryOpts,

    /// Additional input arguments to compile the entry file.
    pub inputs: Dict,

    /// The path to the font profile for cache.
    #[serde(rename = "fontProfileCachePath")]
    pub font_profile_cache_path: PathBuf,

    /// The paths to the font files.
    #[serde(rename = "fontPaths")]
    pub font_paths: Vec<PathBuf>,

    /// Whether to exclude system font paths.
    #[serde(rename = "noSystemFonts")]
    pub no_system_fonts: bool,

    /// Whether to include embedded fonts.
    #[serde(rename = "withEmbeddedFonts")]
    #[serde_as(as = "Vec<AsCowBytes>")]
    pub with_embedded_fonts: Vec<Cow<'static, [u8]>>,

    /// The fixed creation timestamp for the world.
    #[serde(rename = "creationTimestamp")]
    pub creation_timestamp: Option<i64>,
}

/// The options to specify the fonts for the world.
#[serde_as]
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CompileFontOpts {
    /// The paths to the font files.
    #[serde(rename = "fontPaths")]
    pub font_paths: Vec<PathBuf>,

    /// Whether to exclude system font paths.
    #[serde(rename = "noSystemFonts")]
    pub no_system_fonts: bool,

    /// The embedded fonts to include.
    #[serde(rename = "withEmbeddedFonts")]
    #[serde_as(as = "Vec<AsCowBytes>")]
    pub with_embedded_fonts: Vec<Cow<'static, [u8]>>,
}

impl From<CompileOpts> for CompileFontOpts {
    fn from(opts: CompileOpts) -> Self {
        Self {
            font_paths: opts.font_paths,
            no_system_fonts: opts.no_system_fonts,
            with_embedded_fonts: opts.with_embedded_fonts,
        }
    }
}
