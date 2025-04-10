use std::borrow::Cow;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use tinymist_std::AsCowBytes;
use typst::foundations::Dict;

use crate::EntryOpts;

#[serde_as]
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CompileOpts {
    /// Path to entry
    pub entry: EntryOpts,

    /// Additional input arguments to compile the entry file.
    pub inputs: Dict,

    /// Path to font profile for cache
    #[serde(rename = "fontProfileCachePath")]
    pub font_profile_cache_path: PathBuf,

    /// will remove later
    #[serde(rename = "fontPaths")]
    pub font_paths: Vec<PathBuf>,

    /// Exclude system font paths
    #[serde(rename = "noSystemFonts")]
    pub no_system_fonts: bool,

    /// Include embedded fonts
    #[serde(rename = "withEmbeddedFonts")]
    #[serde_as(as = "Vec<AsCowBytes>")]
    pub with_embedded_fonts: Vec<Cow<'static, [u8]>>,
}

#[serde_as]
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CompileFontOpts {
    /// will remove later
    #[serde(rename = "fontPaths")]
    pub font_paths: Vec<PathBuf>,

    /// Exclude system font paths
    #[serde(rename = "noSystemFonts")]
    pub no_system_fonts: bool,

    /// Include embedded fonts
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
