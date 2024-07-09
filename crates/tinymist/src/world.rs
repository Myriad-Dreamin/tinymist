use std::{borrow::Cow, path::PathBuf, sync::Arc};

use comemo::Prehashed;
use serde::{Deserialize, Serialize};
use typst_ts_core::{
    config::{compiler::EntryState, CompileFontOpts as FontOptsInner},
    error::prelude::*,
    font::FontResolverImpl,
    TypstDict,
};

use typst_ts_compiler::{
    font::system::SystemFontSearcher,
    package::http::HttpRegistry,
    vfs::{system::SystemAccessModel, Vfs},
    SystemCompilerFeat, TypstSystemUniverse, TypstSystemWorld,
};

/// Compilation options.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CompileOpts {
    /// Options for each single compilation.
    #[serde(flatten)]
    pub once: CompileOnceOpts,
    /// Compilation options for font.
    #[serde(flatten)]
    pub font: CompileFontOpts,
}

/// Options for a single compilation.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileOnceOpts {
    /// The root directory for compilation routine.
    pub root_dir: PathBuf,
    /// Path to entry
    pub entry: PathBuf,
    /// Additional input arguments to compile the entry file.
    pub inputs: TypstDict,
}

/// Compilation options for font.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileFontOpts {
    /// Path to font profile for cache
    pub font_profile_cache_path: PathBuf,
    /// will remove later
    pub font_paths: Vec<PathBuf>,
    /// Ensures system fonts won't be searched
    pub ignore_system_fonts: bool,
}

/// Compiler feature for LSP world.
pub type LspCompilerFeat = SystemCompilerFeat;
/// LSP universe that spawns LSP worlds.
pub type LspUniverse = TypstSystemUniverse;
/// LSP world.
pub type LspWorld = TypstSystemWorld;
/// Immutable prehashed reference to dictionary.
pub type ImmutDict = Arc<Prehashed<TypstDict>>;

/// Builder for LSP world.
pub struct LspWorldBuilder;

impl LspWorldBuilder {
    /// Create [`LspUniverse`] with the given options.
    /// See [`LspCompilerFeat`] for instantiation details.
    /// See [`CompileOpts`] for available options.
    pub fn build(
        entry: EntryState,
        font_resolver: Arc<FontResolverImpl>,
        inputs: ImmutDict,
    ) -> ZResult<LspUniverse> {
        Ok(LspUniverse::new_raw(
            entry,
            Some(inputs),
            Vfs::new(SystemAccessModel {}),
            HttpRegistry::default(),
            font_resolver,
        ))
    }

    /// Resolve fonts from given options.
    pub(crate) fn resolve_fonts(opts: CompileFontOpts) -> ZResult<FontResolverImpl> {
        let mut searcher = SystemFontSearcher::new();
        searcher.resolve_opts(FontOptsInner {
            font_profile_cache_path: opts.font_profile_cache_path,
            font_paths: opts.font_paths,
            no_system_fonts: opts.ignore_system_fonts,
            with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
        })?;
        Ok(searcher.into())
    }
}
