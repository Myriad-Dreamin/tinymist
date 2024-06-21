use std::{borrow::Cow, path::PathBuf, sync::Arc};

use comemo::Prehashed;
use parking_lot::lock_api::RwLock;
use serde::{Deserialize, Serialize};
use typst_ts_core::{
    config::{compiler::EntryState, CompileFontOpts as FontOptsInner},
    error::prelude::*,
    font::FontResolverImpl,
    FontResolver, TypstDict,
};

use typst_ts_compiler::{
    font::system::SystemFontSearcher,
    package::http::HttpRegistry,
    vfs::{system::SystemAccessModel, Vfs},
    SystemCompilerFeat, TypstSystemUniverse, TypstSystemWorld,
};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CompileOpts {
    #[serde(flatten)]
    pub once: CompileOnceOpts,

    #[serde(flatten)]
    pub font: CompileFontOpts,
}

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

#[derive(Debug, Clone)]
pub struct SharedFontResolver {
    pub inner: Arc<FontResolverImpl>,
}

impl FontResolver for SharedFontResolver {
    fn font(&self, idx: usize) -> Option<typst_ts_core::TypstFont> {
        self.inner.font(idx)
    }
    fn font_book(&self) -> &Prehashed<typst::text::FontBook> {
        self.inner.font_book()
    }
}

impl SharedFontResolver {
    pub fn new(opts: CompileFontOpts) -> ZResult<Self> {
        Ok(Self {
            inner: Arc::new(crate::world::LspWorldBuilder::resolve_fonts(opts)?),
        })
    }

    pub fn font_paths(&self) -> &[PathBuf] {
        self.inner.font_paths()
    }
}

pub type LspCompilerFeat = SystemCompilerFeat;
pub type LspUniverse = TypstSystemUniverse;
pub type LspWorld = TypstSystemWorld;

pub type ImmutDict = Arc<Prehashed<TypstDict>>;

pub struct LspWorldBuilder;

impl LspWorldBuilder {
    /// Create [`LspWorld`] with the given options.
    /// See SystemCompilerFeat for instantiation details.
    /// See [`CompileOpts`] for available options.
    pub fn build(
        entry: EntryState,
        font_resolver: SharedFontResolver,
        inputs: ImmutDict,
    ) -> ZResult<LspUniverse> {
        Ok(LspUniverse::new_raw(
            entry,
            Some(inputs),
            Arc::new(RwLock::new(Vfs::new(SystemAccessModel {}))),
            HttpRegistry::default(),
            font_resolver.inner,
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
