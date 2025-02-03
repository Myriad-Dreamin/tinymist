use std::{borrow::Cow, sync::Arc};

use tinymist_std::{error::prelude::*, ImmutPath};
use tinymist_vfs::{system::SystemAccessModel, ImmutDict, Vfs};
use typst::utils::LazyHash;

use crate::{
    args::{CompileFontArgs, CompilePackageArgs},
    config::{CompileFontOpts, CompileOpts},
    font::{system::SystemFontSearcher, FontResolverImpl},
    package::{http::HttpRegistry, RegistryPathMapper},
    EntryState,
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
    pub fn new(mut opts: CompileOpts) -> Result<Self> {
        let registry: Arc<HttpRegistry> = Arc::default();
        let resolver = Arc::new(RegistryPathMapper::new(registry.clone()));
        let inputs = std::mem::take(&mut opts.inputs);
        Ok(Self::new_raw(
            opts.entry.clone().try_into()?,
            Some(Arc::new(LazyHash::new(inputs))),
            Vfs::new(resolver, SystemAccessModel {}),
            registry,
            Arc::new(Self::resolve_fonts(opts)?),
        ))
    }

    /// Resolve fonts from given options.
    fn resolve_fonts(opts: CompileOpts) -> Result<FontResolverImpl> {
        let mut searcher = SystemFontSearcher::new();
        searcher.resolve_opts(opts.into())?;
        Ok(searcher.into())
    }
}

/// Builders for Typst universe.
pub struct SystemUniverseBuilder;

impl SystemUniverseBuilder {
    /// Create [`TypstSystemUniverse`] with the given options.
    /// See [`LspCompilerFeat`] for instantiation details.
    pub fn build(
        entry: EntryState,
        inputs: ImmutDict,
        font_resolver: Arc<FontResolverImpl>,
        package_registry: HttpRegistry,
    ) -> TypstSystemUniverse {
        let registry = Arc::new(package_registry);
        let resolver = Arc::new(RegistryPathMapper::new(registry.clone()));

        TypstSystemUniverse::new_raw(
            entry,
            Some(inputs),
            Vfs::new(resolver, SystemAccessModel {}),
            registry,
            font_resolver,
        )
    }

    /// Resolve fonts from given options.
    pub fn resolve_fonts(args: CompileFontArgs) -> Result<FontResolverImpl> {
        let mut searcher = SystemFontSearcher::new();
        searcher.resolve_opts(CompileFontOpts {
            font_profile_cache_path: Default::default(),
            font_paths: args.font_paths,
            no_system_fonts: args.ignore_system_fonts,
            with_embedded_fonts: typst_assets::fonts().map(Cow::Borrowed).collect(),
        })?;
        Ok(searcher.into())
    }

    /// Resolve package registry from given options.
    pub fn resolve_package(
        cert_path: Option<ImmutPath>,
        args: Option<&CompilePackageArgs>,
    ) -> HttpRegistry {
        HttpRegistry::new(
            cert_path,
            args.and_then(|args| Some(args.package_path.clone()?.into())),
            args.and_then(|args| Some(args.package_cache_path.clone()?.into())),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::{args::CompileOnceArgs, CompileSnapshot, WorldComputable, WorldComputeGraph};

    use super::*;

    #[test]
    fn compute_system_fonts() {
        pub struct LspFonts {
            fonts: Arc<FontResolverImpl>,
        }

        impl WorldComputable<SystemCompilerFeat> for LspFonts {
            fn compute(graph: &Arc<WorldComputeGraph<SystemCompilerFeat>>) -> Result<Self> {
                Ok(Self {
                    fonts: graph.snap.world.font_resolver.clone(),
                })
            }
        }

        let args = CompileOnceArgs {
            input: Some("main.typ".to_owned()),
            ..Default::default()
        };
        let verse = args.resolve_system().expect("default it");

        let snap = CompileSnapshot::from_world(verse.snapshot());

        let graph = WorldComputeGraph::new(snap);

        let font = graph.compute::<LspFonts>().expect("font").fonts.clone();
        let _ = font;
    }
}
