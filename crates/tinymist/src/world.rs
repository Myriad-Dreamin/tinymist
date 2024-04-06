use std::{borrow::Cow, path::PathBuf, sync::Arc};

use comemo::Prehashed;
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
    world::CompilerWorld,
};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CompileOpts {
    #[serde(flatten)]
    pub once: CompileOnceOpts,

    #[serde(flatten)]
    pub font: CompileFontOpts,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CompileOnceOpts {
    /// The root directory for compilation routine.
    #[serde(rename = "rootDir")]
    pub root_dir: PathBuf,

    /// Path to entry
    pub entry: PathBuf,

    /// Additional input arguments to compile the entry file.
    pub inputs: TypstDict,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CompileFontOpts {
    /// Path to font profile for cache
    #[serde(rename = "fontProfileCachePath")]
    pub font_profile_cache_path: PathBuf,

    /// will remove later
    #[serde(rename = "fontPaths")]
    pub font_paths: Vec<PathBuf>,

    /// Exclude system font paths
    #[serde(rename = "noSystemFonts")]
    pub no_system_fonts: bool,
}

#[derive(Debug, Clone)]
pub struct SharedFontResolver {
    font_paths: Vec<PathBuf>,
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
    pub fn new(mut opts: CompileFontOpts) -> ZResult<Self> {
        // todo: relative paths?
        let mut has_relative_path = false;
        for p in &opts.font_paths {
            if p.is_relative() {
                has_relative_path = true;
                break;
            }
        }
        if has_relative_path {
            let current_dir = std::env::current_dir()
                .context("failed to get current directory for relative font paths")?;
            for p in &mut opts.font_paths {
                if p.is_relative() {
                    *p = current_dir.join(&p);
                }
            }
        }

        let font_paths = opts.font_paths.clone();
        let res = crate::world::LspWorldBuilder::resolve_fonts(opts)?;
        Ok(Self {
            font_paths,
            inner: Arc::new(res),
        })
    }

    pub fn font_paths(&self) -> &[PathBuf] {
        &self.font_paths
    }
}

/// type trait of [`LspWorld`].
#[derive(Debug, Clone, Copy)]
pub struct SystemCompilerFeat;

impl typst_ts_compiler::world::CompilerFeat for SystemCompilerFeat {
    /// Uses [`SharedFontResolver`] directly.
    type FontResolver = SharedFontResolver;
    /// It accesses a physical file system.
    type AccessModel = SystemAccessModel;
    /// It performs native HTTP requests for fetching package data.
    type Registry = HttpRegistry;
}

/// The compiler world in system environment.
pub type LspWorld = CompilerWorld<SystemCompilerFeat>;

pub type ImmutDict = Arc<Prehashed<TypstDict>>;

pub struct LspWorldBuilder;
// Self::resolve_fonts(opts)?,

impl LspWorldBuilder {
    /// Create [`LspWorld`] with the given options.
    /// See SystemCompilerFeat for instantiation details.
    /// See [`CompileOpts`] for available options.
    pub fn build(
        entry: EntryState,
        font_resolver: SharedFontResolver,
        inputs: ImmutDict,
    ) -> ZResult<LspWorld> {
        let mut res = CompilerWorld::new_raw(
            entry,
            Vfs::new(SystemAccessModel {}),
            HttpRegistry::default(),
            font_resolver,
        );
        res.inputs = inputs;
        Ok(res)
    }

    /// Resolve fonts from given options.
    pub(crate) fn resolve_fonts(opts: CompileFontOpts) -> ZResult<FontResolverImpl> {
        let mut searcher = SystemFontSearcher::new();
        searcher.resolve_opts(FontOptsInner {
            font_profile_cache_path: opts.font_profile_cache_path,
            font_paths: opts.font_paths,
            no_system_fonts: opts.no_system_fonts,
            with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
        })?;
        Ok(searcher.into())
    }
}
