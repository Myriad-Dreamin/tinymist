use std::sync::Arc;

use reflexo::error::prelude::*;
use reflexo_vfs::{system::SystemAccessModel, Vfs};
use typst::utils::LazyHash;

use crate::{
    config::CompileOpts,
    font::{system::SystemFontSearcher, FontResolverImpl},
    package::http::HttpRegistry,
};

/// type trait of [`TypstSystemWorld`].
#[derive(Debug, Clone, Copy)]
pub struct SystemCompilerFeat;

impl crate::CompilerFeat for SystemCompilerFeat {
    /// Uses [`FontResolverImpl`] directly.
    type FontResolver = FontResolverImpl;
    /// It accesses a physical file system.
    type AccessModel = SystemAccessModel;
    /// It performs native HTTP requests for fetching package data.
    type Registry = HttpRegistry;
}

/// The compiler universe in system environment.
pub type TypstSystemUniverse = crate::world::CompilerUniverse<SystemCompilerFeat>;
/// The compiler world in system environment.
pub type TypstSystemWorld = crate::world::CompilerWorld<SystemCompilerFeat>;

impl TypstSystemUniverse {
    /// Create [`TypstSystemWorld`] with the given options.
    /// See SystemCompilerFeat for instantiation details.
    /// See [`CompileOpts`] for available options.
    pub fn new(mut opts: CompileOpts) -> ZResult<Self> {
        let inputs = std::mem::take(&mut opts.inputs);
        Ok(Self::new_raw(
            opts.entry.clone().try_into()?,
            Some(Arc::new(LazyHash::new(inputs))),
            Vfs::new(SystemAccessModel {}),
            HttpRegistry::default(),
            Arc::new(Self::resolve_fonts(opts)?),
        ))
    }

    /// Resolve fonts from given options.
    fn resolve_fonts(opts: CompileOpts) -> ZResult<FontResolverImpl> {
        let mut searcher = SystemFontSearcher::new();
        searcher.resolve_opts(opts.into())?;
        Ok(searcher.into())
    }
}
